"""Keep the LAN access code encrypted with the current Windows user's DPAPI key."""
import base64
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path

SETTINGS = Path(os.environ.get('LOCALAPPDATA', Path.home())) / 'BambuDiscord' / 'settings.json'


class Blob(ctypes.Structure):
    _fields_ = [('size', wintypes.DWORD), ('data', ctypes.POINTER(ctypes.c_ubyte))]


def crypt(data, decrypt=False):
    buf = ctypes.create_string_buffer(data)
    source = Blob(len(data), ctypes.cast(buf, ctypes.POINTER(ctypes.c_ubyte)))
    result = Blob()
    api = ctypes.windll.crypt32
    if decrypt:
        ok = api.CryptUnprotectData(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(result))
    else:
        ok = api.CryptProtectData(ctypes.byref(source), 'BambuDiscord', None, None, None, 1, ctypes.byref(result))
    if not ok:
        raise ctypes.WinError()
    try:
        return ctypes.string_at(result.data, result.size)
    finally:
        ctypes.windll.kernel32.LocalFree.argtypes = [ctypes.c_void_p]
        ctypes.windll.kernel32.LocalFree(result.data)


def save(config, path=SETTINGS):
    stored = dict(config)
    stored['access_code'] = base64.b64encode(crypt(config['access_code'].encode())).decode()
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix('.tmp')
    temp.write_text(json.dumps(stored, indent=2), encoding='utf-8')
    temp.replace(path)


def load(path=SETTINGS):
    if not path.exists():
        return {}
    stored = json.loads(path.read_text(encoding='utf-8'))
    stored['access_code'] = crypt(base64.b64decode(stored['access_code']), decrypt=True).decode()
    return stored
