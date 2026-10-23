"""Bambu telemetry -> Discord activity. No printer control operations."""
import asyncio
import json
import math
from pathlib import Path
import re
import ssl
import threading
import time

import paho.mqtt.client as mqtt
from pypresence import Presence

PRINTER_IMAGE_URL = ('https://raw.githubusercontent.com/bambulab/BambuStudio/'
                     'master/resources/images/printer_preview_N2S.png')


class DiscordPresence(Presence):
    """Bound handshake time and retain the worker's loop across reconnects."""
    def connect(self):
        return self.loop.run_until_complete(asyncio.wait_for(self.handshake(), timeout=5))

    def close(self):
        if self.sock_writer is not None:
            self.sock_writer.close()
            self.loop.run_until_complete(asyncio.sleep(0))


def number(value, maximum):
    try:
        value = float(value)
        return int(value) if math.isfinite(value) and 0 <= value <= maximum else None
    except (TypeError, ValueError, OverflowError):
        return None


def validate(config):
    if not re.fullmatch(r"(?:\d{1,3}\.){3}\d{1,3}", config['host']):
        raise ValueError('Yazıcının yerel IPv4 adresini girin.')
    import ipaddress
    ip = ipaddress.ip_address(config['host'])
    if not ip.is_private or ip.is_loopback or ip.is_unspecified or ip.is_multicast:
        raise ValueError('Yazıcının yerel ağdaki IP adresini girin.')
    if not re.fullmatch(r'[A-Za-z0-9]{8}', config['access_code']):
        raise ValueError('LAN erişim kodu 8 harf/rakam olmalı.')
    if not re.fullmatch(r'[A-Za-z0-9]{8,32}', config['serial']):
        raise ValueError('Yazıcının seri numarasını kontrol edin.')
    if not re.fullmatch(r'\d{17,22}', config['app_id']):
        raise ValueError('Discord Application ID alanına uygulamanın sayısal kimliğini girin.')


class Telemetry:
    def __init__(self):
        self.data = {}
        self.last_seen = None
        self.remaining_at = None
        self.remaining = None

    def ingest(self, packet, now):
        data = packet.get('print') if isinstance(packet, dict) else None
        if not isinstance(data, dict) or not data:
            return
        # A1 sends deltas; retain fields omitted from later packets.
        for identity in ('subtask_id', 'task_id', 'subtask_name'):
            old, new = self.data.get(identity), data.get(identity)
            if new and old and new != old:
                self.data.clear()
                self.remaining = self.remaining_at = None
                break
        old_state = self.data.get('gcode_state')
        new_state = data.get('gcode_state')
        if new_state in ('RUNNING', 'PREPARE') and old_state in ('FINISH', 'FAILED', 'IDLE'):
            self.data.clear()
            self.remaining = self.remaining_at = None
        self.data.update(data)
        if 'mc_remaining_time' in data:
            self.remaining = number(data['mc_remaining_time'], 100000)
            self.remaining_at = now
        elif new_state == 'RUNNING' and old_state == 'PAUSE' and self.remaining is not None:
            self.remaining_at = now
        self.last_seen = now

    def activity(self, now, show_name=True):
        if self.last_seen is None or now - self.last_seen > 90:
            return None
        state = self.data.get('gcode_state')
        if state not in ('RUNNING', 'PREPARE', 'PAUSE'):
            return None
        name = self.data.get('subtask_name') or self.data.get('gcode_file')
        details = str(name).strip() if name and show_name else '3D baskı alıyorum'
        details = details.replace('\\', '/').rsplit('/', 1)[-1][:100]
        if len(details) < 2:
            details = '3D baskı alıyorum'
        percent = number(self.data.get('mc_percent'), 100)
        label = {'RUNNING': 'Basılıyor', 'PREPARE': 'Hazırlanıyor', 'PAUSE': 'Duraklatıldı'}[state]
        parts = ['A1 Combo', label]
        if percent is not None:
            parts.append(f'%{percent}')
        if self.remaining is not None and state == 'RUNNING':
            elapsed = max(0, now - self.remaining_at) / 60
            minutes = max(0, math.ceil(self.remaining - elapsed))
            if minutes:
                hours, mins = divmod(minutes, 60)
                parts.append((f'{hours} sa ' if hours else '') + f'{mins} dk kaldı')
        return {'details': details, 'state': ' · '.join(parts),
                'large_image': PRINTER_IMAGE_URL, 'large_text': 'Bambu Lab A1 Combo'}


class Bridge:
    def __init__(self, config, emit):
        self.config, self.emit = config, emit
        self.stop_event = threading.Event()
        self.lock = threading.Lock()
        self.telemetry = Telemetry()
        self.connected = False
        self.client = None
        self.thread = threading.Thread(target=self.run, daemon=True)

    def start(self):
        self.thread.start()

    def stop(self):
        self.stop_event.set()

    def on_connect(self, client, userdata, flags, reason, properties):
        with self.lock:
            self.telemetry = Telemetry()
            self.connected = not reason.is_failure
        if reason.is_failure:
            self.emit('printer', 'Yazıcı bağlantıyı reddetti. LAN erişim kodunu kontrol edin.')
            return
        client.subscribe(f"device/{self.config['serial']}/report")
        self.emit('printer', 'Bağlandı; baskı bilgisi bekleniyor.')

    def on_subscribe(self, client, userdata, mid, reasons, properties):
        if any(r.is_failure for r in reasons):
            self.emit('printer', 'Durum kanalına erişilemedi. Seri numarasını kontrol edin.')
            return
        self.request_snapshot()

    def request_snapshot(self):
        # Read-only status request, needed to bootstrap A1 delta reports.
        self.client.publish(f"device/{self.config['serial']}/request",
                            json.dumps({'pushing': {'sequence_id': '0', 'command': 'pushall'}}))

    def on_disconnect(self, client, userdata, flags, reason, properties):
        with self.lock:
            self.connected = False
            self.telemetry = Telemetry()
        if not self.stop_event.is_set():
            self.emit('printer', 'Bağlantı kesildi; yeniden deneniyor.')

    def on_message(self, client, userdata, message):
        if len(message.payload) > 1024 * 1024:
            return
        try:
            packet = json.loads(message.payload)
        except (ValueError, UnicodeError):
            return
        with self.lock:
            self.telemetry.ingest(packet, time.monotonic())

    def run(self):
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        rpc = None
        shown = False
        next_rpc = next_update = next_snapshot = 0
        try:
            context = ssl.create_default_context(cafile=str(Path(__file__).with_name('bambu.cert')))
            # Printer certificates identify the device, not its DHCP IP address.
            # Validate the manufacturer CA; never disable certificate verification.
            context.check_hostname = False
            context.minimum_version = ssl.TLSVersion.TLSv1_2
            context.maximum_version = ssl.TLSVersion.TLSv1_2
            context.verify_flags &= ~ssl.VERIFY_X509_STRICT
            self.client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)
            self.client.username_pw_set('bblp', self.config['access_code'])
            self.client.tls_set_context(context)
            self.client.on_connect = self.on_connect
            self.client.on_subscribe = self.on_subscribe
            self.client.on_disconnect = self.on_disconnect
            self.client.on_message = self.on_message
            self.client.on_connect_fail = lambda *_: self.emit(
                'printer', 'Yazıcıya ulaşılamadı. IP, aynı ağ ve LAN erişimini kontrol edin.')
            self.client.reconnect_delay_set(2, 30)
            self.client.connect_async(self.config['host'], 8883, 30)
            self.client.loop_start()
            self.emit('printer', 'Yazıcıya bağlanılıyor…')
            while not self.stop_event.is_set():
                now = time.monotonic()
                with self.lock:
                    activity = self.telemetry.activity(now, self.config.get('show_name', True)) if self.connected else None
                    stale = self.connected and (self.telemetry.last_seen is None or now - self.telemetry.last_seen > 90)
                if self.connected and now >= next_snapshot:
                    self.request_snapshot()
                    next_snapshot = now + 60
                self.emit('preview', activity)
                if stale:
                    self.emit('printer', 'Baskı verisi bekleniyor. Seri numarası / LAN erişimini kontrol edin.')
                elif self.connected:
                    self.emit('printer', 'Yazıcı bağlı · ' + ('Baskı sürüyor' if activity else 'Aktif baskı yok'))
                try:
                    if rpc is None and now >= next_rpc:
                        rpc = DiscordPresence(self.config['app_id'], loop=loop,
                                              connection_timeout=5, response_timeout=3)
                        rpc.connect()
                        next_update = 0
                        self.emit('discord', 'Discord bağlı')
                    if rpc is not None:
                        if activity and now >= next_update:
                            rpc.update(**activity)
                            shown = True
                            next_update = now + 15
                            self.emit('discord', 'Baskı durumu Discord’a gönderildi')
                        elif activity is None and shown:
                            rpc.clear()
                            shown = False
                            self.emit('discord', 'Discord bağlı · Durum temizlendi')
                except Exception:
                    if rpc is not None:
                        try:
                            rpc.close()
                        except Exception:
                            pass
                    rpc = None
                    shown = False
                    next_rpc = now + 15
                    self.emit('discord', 'Discord bağlantısı yok. Masaüstü uygulaması ve Application ID’yi kontrol edin.')
                self.stop_event.wait(1)
        except Exception as exc:
            # Never report exception arguments: credentials could appear in library errors.
            self.emit('error', f'Bağlantı başlatılamadı ({type(exc).__name__}). Ayarları kontrol edin.')
        finally:
            if self.client is not None:
                self.client.disconnect()
                self.client.loop_stop()
            if rpc is not None:
                try:
                    rpc.clear()
                    rpc.close()
                except Exception:
                    pass
            loop.close()
            self.emit('stopped', None)
