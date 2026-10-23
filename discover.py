"""Read the selected Studio device and discover its local SSDP announcement."""
import json
import os
from pathlib import Path
import select
import socket
import time

import settings


def from_studio():
    path = Path(os.environ['APPDATA']) / 'BambuStudio' / 'BambuStudio.conf'
    data, _ = json.JSONDecoder().raw_decode(path.read_text(encoding='utf-8').lstrip())
    codes = data.get('access_code', {})
    user_codes = data.get('user_access_code', {})
    serials = [serial for serial in set(codes) | set(user_codes) if serial.startswith('039')]
    if len(serials) != 1:
        raise ValueError('Studio içinde tek bir A1 seçilemedi.')
    serial = serials[0]
    code = codes.get(serial) or user_codes.get(serial)
    if not isinstance(code, str) or len(code) != 8:
        raise ValueError('Studio içindeki kod uygun biçimde değil.')
    return {'serial': serial, 'access_code': code}


def discover(serial, timeout=15):
    sockets = []
    sender = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        for port in (1990, 2021):
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            try:
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                sock.bind(('', port))
                sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP,
                                socket.inet_aton('239.255.255.250') + socket.inet_aton('0.0.0.0'))
                sock.setblocking(False)
                sockets.append(sock)
            except OSError:
                sock.close()
        sender.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, 2)
        query = ('M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1990\r\n'
                 'MAN: "ssdp:discover"\r\nMX: 1\r\n'
                 'ST: urn:bambulab-com:device:3dprinter:1\r\n\r\n').encode()
        for port in (1990, 2021):
            sender.sendto(query, ('239.255.255.250', port))
        end = time.monotonic() + timeout
        while sockets and time.monotonic() < end:
            ready, _, _ = select.select(sockets, [], [], 1)
            for sock in ready:
                packet, address = sock.recvfrom(16384)
                text = packet.decode(errors='replace')
                headers = {k.strip().lower(): v.strip() for line in text.splitlines()
                           if ':' in line for k, v in [line.split(':', 1)]}
                if headers.get('usn') == serial:
                    return address[0]
        return ''
    finally:
        for sock in sockets:
            sock.close()
        sender.close()


if __name__ == '__main__':
    config = settings.load()
    config.update(from_studio())
    print('A1 seri numarası ve erişim kodu bulundu; kod gösterilmiyor.', flush=True)
    host = discover(config['serial'])
    if host:
        config['host'] = host
    config.setdefault('app_id', '')
    config.setdefault('show_name', True)
    settings.save(config)
    print('IP adresi: ' + (host or 'Yerel ağ duyurusunda bulunamadı.'))
