import asyncio
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, MagicMock

import core
import settings


class TelemetryTests(unittest.TestCase):
    def setUp(self):
        self.t = core.Telemetry()
        self.t.ingest({'print': {'gcode_state': 'RUNNING', 'mc_percent': 64,
                                'mc_remaining_time': 42, 'subtask_name': 'Benchy'}}, 100)

    def test_delta_preserves_state_and_name(self):
        self.t.ingest({'print': {'mc_percent': 65}}, 110)
        activity = self.t.activity(110)
        self.assertEqual(activity['details'], 'Benchy')
        self.assertIn('%65', activity['state'])
        self.assertIn('42 dk kaldı', activity['state'])

    def test_pause_has_no_countdown(self):
        self.t.ingest({'print': {'gcode_state': 'PAUSE'}}, 110)
        self.assertIn('Duraklatıldı', self.t.activity(110)['state'])
        self.assertNotIn('kaldı', self.t.activity(110)['state'])

    def test_finish_failure_idle_remove_activity(self):
        for state in ('FINISH', 'FAILED', 'IDLE', 'UNKNOWN'):
            self.t.ingest({'print': {'gcode_state': state}}, 110)
            self.assertIsNone(self.t.activity(110))

    def test_stale_data_is_hidden(self):
        self.assertIsNone(self.t.activity(191))

    def test_new_job_does_not_reuse_old_progress(self):
        self.t.ingest({'print': {'subtask_name': 'New print', 'gcode_state': 'PREPARE'}}, 110)
        activity = self.t.activity(110)
        self.assertNotIn('%64', activity['state'])
        self.assertNotIn('kaldı', activity['state'])

    def test_name_privacy(self):
        self.assertEqual(self.t.activity(100, False)['details'], '3D baskı alıyorum')

    def test_invalid_progress_and_time_are_not_displayed(self):
        for value in (None, 'NaN', -2, 'nonsense', float('inf'), 100001):
            self.t.ingest({'print': {'mc_percent': value, 'mc_remaining_time': value}}, 110)
            state = self.t.activity(110)['state']
            self.assertNotIn('%', state)
            self.assertNotIn('kaldı', state)

    def test_unrelated_packets_do_not_keep_dead_data_alive(self):
        for packet in (None, [], {'info': {}}, {'print': []}, {'print': {}}):
            self.t.ingest(packet, 190)
        self.assertIsNone(self.t.activity(191))

    def test_time_does_not_jump_back_on_delta(self):
        self.t.ingest({'print': {'mc_percent': 70}}, 170)
        self.assertIn('41 dk kaldı', self.t.activity(170)['state'])


class SettingsTests(unittest.TestCase):
    def test_dpapi_round_trip_and_no_plaintext_on_disk(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'settings.json'
            config = {'access_code': 'ABCDEFGH', 'host': '192.168.1.3'}
            settings.save(config, path)
            self.assertNotIn('ABCDEFGH', path.read_text())
            self.assertEqual(settings.load(path), config)

    def test_validation(self):
        config = dict(host='192.168.1.3', access_code='ABCDEFGH', serial='0300ABCDEF12345', app_id='123456789012345678')
        core.validate(config)
        for key, value in [('host', '8.8.8.8'), ('host', '127.0.0.1'), ('host', '999.1.1.1'),
                           ('serial', 'abc/#'), ('app_id', 'bot token'), ('access_code', 'short')]:
            with self.assertRaises(ValueError):
                core.validate({**config, key: value})


class BridgeTests(unittest.TestCase):
    def test_clear_on_disconnect_and_shutdown(self):
        events = []
        b = core.Bridge(dict(host='192.168.1.3', access_code='ABCDEFGH', serial='0300ABCDEF12345',
                             app_id='123456789012345678'), lambda *args: events.append(args))
        client = MagicMock()
        rpc = MagicMock()
        clock = [100.0]
        tick = [0]

        def connected():
            reason = MagicMock(is_failure=False)
            b.on_connect(client, None, None, reason, None)
            b.on_subscribe(client, None, 1, [reason], None)
            b.on_message(client, None, MagicMock(payload=json.dumps({'print': {
                'gcode_state': 'RUNNING', 'mc_percent': 50, 'subtask_name': 'Test'}}).encode()))

        def wait(_):
            tick[0] += 1
            clock[0] += 16
            if tick[0] == 1:
                b.on_disconnect(client, None, None, None, None)
            else:
                b.stop_event.set()

        client.loop_start.side_effect = connected
        with patch.object(core.mqtt, 'Client', return_value=client), \
             patch.object(core, 'DiscordPresence', return_value=rpc), \
             patch.object(core.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(b.stop_event, 'wait', side_effect=wait):
            b.run()
        rpc.update.assert_called_once()
        self.assertGreaterEqual(rpc.clear.call_count, 1)
        client.disconnect.assert_called_once()
        rpc.close.assert_called_once()
        self.assertEqual(events[-1], ('stopped', None))
        # Outbound MQTT consists solely of status snapshot requests.
        for call in client.publish.call_args_list:
            self.assertEqual(json.loads(call.args[1]), {'pushing': {'sequence_id': '0', 'command': 'pushall'}})

    def test_reconnect_after_discord_unavailable(self):
        b = core.Bridge(dict(host='192.168.1.3', access_code='ABCDEFGH', serial='0300ABCDEF12345',
                             app_id='123456789012345678'), lambda *_: None)
        bad, good = MagicMock(), MagicMock()
        bad.connect.side_effect = RuntimeError('Discord closed')
        clock = [100.0]

        def wait(_):
            if clock[0] > 100:
                b.stop_event.set()
            clock[0] += 16

        with patch.object(core.mqtt, 'Client'), \
             patch.object(core, 'DiscordPresence', side_effect=[bad, good]) as factory, \
             patch.object(core.time, 'monotonic', side_effect=lambda: clock[0]), \
             patch.object(b.stop_event, 'wait', side_effect=wait):
            b.run()
        self.assertEqual(factory.call_count, 2)
        good.connect.assert_called_once()

    def test_discord_handshake_timeout(self):
        loop = asyncio.new_event_loop()
        try:
            presence = core.DiscordPresence('123456789012345678', loop=loop)
            async def handshake():
                await asyncio.sleep(1)
            presence.handshake = handshake
            original = asyncio.wait_for
            with patch.object(core.asyncio, 'wait_for', new=lambda coro, timeout: original(coro, .01)):
                with self.assertRaises(TimeoutError):
                    presence.connect()
            presence.close()
            self.assertFalse(loop.is_closed())
        finally:
            loop.close()


if __name__ == '__main__':
    unittest.main()
