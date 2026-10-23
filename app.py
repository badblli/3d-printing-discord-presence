import queue
import os
import sys
import threading
import tkinter as tk
from tkinter import messagebox, ttk
import webbrowser

from PIL import Image, ImageTk
import pystray

from core import Bridge, validate
import settings


class App:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title('3D Printing')
        self.root.geometry('760x850')
        self.root.minsize(700, 780)
        self.events = queue.Queue()
        self.bridge = None
        self.tray = None
        self.closing = False
        try:
            config = settings.load()
        except Exception:
            config = {}
            messagebox.showwarning('Ayarlar okunamadı', 'Kayıtlı ayarlar okunamadı. Bilgileri yeniden girin.')
        style = ttk.Style()
        style.theme_use('clam')
        self.colors = {
            'bg': '#0d1117', 'surface': '#151c25', 'surface2': '#1c2632',
            'border': '#293545', 'text': '#f1f5f9', 'muted': '#9aa9b9',
            'accent': '#52dda0', 'blue': '#8aa7ff', 'danger': '#ff7b72',
        }
        self.root.configure(bg=self.colors['bg'])
        try:
            icon_path = self.resource_path('assets/app-icon-3d-printing-v1.png')
            self.window_icon = ImageTk.PhotoImage(Image.open(icon_path).resize((64, 64), Image.Resampling.LANCZOS))
            self.root.iconphoto(True, self.window_icon)
        except Exception:
            self.window_icon = None
        style.configure('TFrame', background=self.colors['bg'])
        style.configure('Card.TFrame', background=self.colors['surface'])
        style.configure('Inner.TFrame', background=self.colors['surface2'])
        style.configure('TLabel', background=self.colors['bg'], foreground=self.colors['text'], font=('Segoe UI', 10))
        style.configure('Card.TLabel', background=self.colors['surface'], foreground=self.colors['text'], font=('Segoe UI', 10))
        style.configure('Muted.TLabel', background=self.colors['surface'], foreground=self.colors['muted'], font=('Segoe UI', 9))
        style.configure('Hero.TLabel', background=self.colors['bg'], foreground=self.colors['text'], font=('Segoe UI', 23, 'bold'))
        style.configure('Section.TLabel', background=self.colors['surface'], foreground=self.colors['text'], font=('Segoe UI', 12, 'bold'))
        style.configure('TButton', font=('Segoe UI', 10, 'bold'), padding=(13, 9), background=self.colors['surface2'], foreground=self.colors['text'], borderwidth=0)
        style.map('TButton', background=[('active', '#2a3949'), ('disabled', '#202833')], foreground=[('disabled', '#718091')])
        style.configure('Accent.TButton', background=self.colors['accent'], foreground='#092018')
        style.map('Accent.TButton', background=[('active', '#70edb4'), ('disabled', '#315d4b')], foreground=[('disabled', '#9bb8aa')])
        style.configure('TCheckbutton', background=self.colors['surface'], foreground=self.colors['text'], font=('Segoe UI', 10))
        style.map('TCheckbutton', background=[('active', self.colors['surface'])], foreground=[('disabled', self.colors['muted'])])
        style.configure('TEntry', fieldbackground='#0d131b', foreground=self.colors['text'], insertcolor=self.colors['text'], bordercolor=self.colors['border'], lightcolor=self.colors['border'], darkcolor=self.colors['border'], padding=9)
        main = ttk.Frame(self.root, padding=(28, 22))
        main.pack(fill='both', expand=True)
        header = ttk.Frame(main)
        header.pack(fill='x', pady=(0, 18))
        self.header_icon = None
        if self.window_icon:
            self.header_icon = ImageTk.PhotoImage(Image.open(icon_path).resize((58, 58), Image.Resampling.LANCZOS))
            ttk.Label(header, image=self.header_icon).pack(side='left', padx=(0, 14))
        title_group = ttk.Frame(header)
        title_group.pack(side='left', fill='y', pady=3)
        ttk.Label(title_group, text='3D Printing', style='Hero.TLabel').pack(anchor='w')
        ttk.Label(title_group, text='Bambu Lab A1 Combo  ·  Discord Rich Presence', foreground=self.colors['muted']).pack(anchor='w', pady=(2, 0))
        ttk.Label(header, text='LOCAL  ·  LAN', foreground=self.colors['accent'], background=self.colors['surface'], padding=(10, 6), font=('Segoe UI', 8, 'bold')).pack(side='right', anchor='n', pady=9)

        status = ttk.Frame(main, style='Card.TFrame', padding=17)
        status.pack(fill='x', pady=(0, 14))
        status_top = ttk.Frame(status, style='Card.TFrame')
        status_top.pack(fill='x')
        ttk.Label(status_top, text='BAĞLANTI DURUMU', style='Muted.TLabel').pack(side='left')
        self.status_pill = ttk.Label(status_top, text='●  BAĞLI DEĞİL', foreground=self.colors['muted'], background=self.colors['surface'], font=('Segoe UI', 9, 'bold'))
        self.status_pill.pack(side='right')
        self.printer = tk.StringVar(value='Yazıcıya henüz bağlanılmadı')
        self.discord = tk.StringVar(value='Discord bağlantısı bekleniyor')
        self.preview = tk.StringVar(value='Aktif baskı yokken Discord durumu gösterilmez.')
        ttk.Label(status, textvariable=self.printer, style='Card.TLabel', wraplength=630).pack(anchor='w', pady=(13, 3))
        ttk.Label(status, textvariable=self.discord, style='Muted.TLabel', wraplength=630).pack(anchor='w', pady=(0, 11))
        preview_box = ttk.Frame(status, style='Inner.TFrame', padding=(12, 10))
        preview_box.pack(fill='x')
        ttk.Label(preview_box, text='DISCORD ÖNİZLEMESİ', foreground=self.colors['accent'], background=self.colors['surface2'], font=('Segoe UI', 8, 'bold')).pack(anchor='w')
        ttk.Label(preview_box, textvariable=self.preview, foreground=self.colors['text'], background=self.colors['surface2'], font=('Segoe UI', 10, 'bold'), wraplength=620, justify='left').pack(anchor='w', pady=(5, 0))

        config_card = ttk.Frame(main, style='Card.TFrame', padding=18)
        config_card.pack(fill='both', expand=True)
        section_header = ttk.Frame(config_card, style='Card.TFrame')
        section_header.pack(fill='x', pady=(0, 13))
        ttk.Label(section_header, text='Bağlantı ayarları', style='Section.TLabel').pack(side='left')
        ttk.Label(section_header, text='Bilgiler bu bilgisayarda saklanır', style='Muted.TLabel').pack(side='right', anchor='s')
        self.fields = {}
        self.entries = []
        for key, title, hint in [
            ('host', 'Yazıcı IP adresi', 'Yazıcı ekranı → Ayarlar → WLAN / Ağ. Örnek: 192.168.1.50'),
            ('serial', 'Yazıcı seri numarası', 'Yazıcı ekranı → Ayarlar → Cihaz bilgisi. AMS seri numarası değil.'),
            ('access_code', 'LAN erişim kodu', 'WLAN / Ağ → LAN erişim kodu. Bambu hesap şifreniz değil.'),
            ('app_id', 'Discord Application ID', 'Developer Portal → New Application → General Information → Application ID.'),
        ]:
            ttk.Label(config_card, text=title, style='Card.TLabel', font=('Segoe UI', 10, 'bold')).pack(anchor='w')
            var = tk.StringVar(value=config.get(key, ''))
            self.fields[key] = var
            entry = ttk.Entry(main, textvariable=var, font=('Segoe UI', 11), show='•' if key == 'access_code' else '')
            entry.pack(fill='x', pady=(3, 3))
            self.entries.append(entry)
            ttk.Label(config_card, text=hint, style='Muted.TLabel', wraplength=660).pack(anchor='w', pady=(0, 10))
        portal = ttk.Label(config_card, text='Discord Developer Portal’ı aç  ↗', foreground=self.colors['blue'], background=self.colors['surface'], cursor='hand2', font=('Segoe UI', 9, 'bold'))
        portal.pack(anchor='w')
        portal.bind('<Button-1>', lambda _: webbrowser.open('https://discord.com/developers/applications'))
        self.show_name = tk.BooleanVar(value=config.get('show_name', True))
        self.check = ttk.Checkbutton(config_card, text='Basılan dosyanın adını Discord’da göster', variable=self.show_name)
        self.check.pack(anchor='w', pady=(13, 12))
        actions = ttk.Frame(config_card, style='Card.TFrame')
        actions.pack(fill='x')
        self.start_button = ttk.Button(actions, text='Kaydet ve başlat', style='Accent.TButton', command=self.start)
        self.start_button.pack(side='left')
        self.stop_button = ttk.Button(actions, text='Durdur', command=self.stop, state='disabled')
        self.stop_button.pack(side='left', padx=8)
        ttk.Button(actions, text='Arka plana al', command=self.hide).pack(side='right')
        ttk.Label(config_card, text='LAN kodu Windows hesabınızla şifrelenerek saklanır. Discord masaüstü açık olmalı; Etkinlik Gizliliği’nde etkinlik paylaşımını açın.', style='Muted.TLabel', wraplength=650, justify='left').pack(anchor='w', pady=(14, 0))
        self.root.protocol('WM_DELETE_WINDOW', self.hide)
        menu = tk.Menu(self.root)
        menu.add_command(label='Kurulum yardımı', command=self.help)
        menu.add_command(label='Tamamen çık', command=self.quit)
        self.root.config(menu=menu)
        self.root.after(200, self.poll)

    @staticmethod
    def resource_path(relative):
        base = getattr(sys, '_MEIPASS', os.path.dirname(os.path.abspath(__file__)))
        return os.path.join(base, relative)

    def help(self):
        messagebox.showinfo('İlk kurulum',
            '1. Discord Developer Portal’da New Application ile "3D Baskı" adlı bir uygulama oluşturun. '
            'General Information bölümündeki Application ID’yi buraya kopyalayın. Bot oluşturmanız gerekmez.\n\n'
            '2. Yazıcının IP’sini, yazıcı seri numarasını ve LAN erişim kodunu girin. '
            'Erişim kodunu Discord’a veya sohbete göndermeyin.\n\n'
            '3. Discord masaüstünü açın. Ayarlar → Etkinlik Gizliliği altında etkinlik paylaşımını açın. '
            'Kaydet ve başlat’a basın.\n\n'
            'Önce mevcut ağ ayarlarınızla deneyin. Durum alınamıyorsa kodu, seri numarasını ve yazıcı sürümünün '
            'LAN erişim ayarlarını kontrol edin. LAN Only modu açılırsa Bambu Handy bulut bağlantısı kesilir.\n\n'
            'Pencereyi kapatınca uygulama bildirim alanında çalışır. Tamamen çıkmak için menüyü kullanın.')

    def start(self):
        config = {key: var.get().strip() for key, var in self.fields.items()}
        config['show_name'] = self.show_name.get()
        try:
            validate(config)
            settings.save(config)
        except Exception as exc:
            messagebox.showerror('Ayarlar', str(exc) if isinstance(exc, ValueError) else 'Ayarlar kaydedilemedi.')
            return
        self.bridge = Bridge(config, lambda key, value: self.events.put((key, value)))
        self.set_running(True)
        self.bridge.start()

    def set_running(self, running):
        self.start_button.config(state='disabled' if running else 'normal')
        self.stop_button.config(state='normal' if running else 'disabled')
        for entry in self.entries:
            entry.config(state='disabled' if running else 'normal')
        self.check.config(state='disabled' if running else 'normal')

    def stop(self):
        if self.bridge:
            self.stop_button.config(state='disabled')
            self.bridge.stop()
            self.discord.set('Durduruluyor; Discord durumu temizleniyor…')

    def poll(self):
        try:
            while True:
                key, value = self.events.get_nowait()
                if key == 'show':
                    self.root.deiconify()
                    self.root.lift()
                elif key == 'quit':
                    self.quit()
                elif key == 'printer':
                    self.printer.set(value)
                    status = value.lower()
                    self.set_connection_state('connected' if 'bağlandı' in status or 'yazıcı bağlı' in status or 'connected' in status else 'warning')
                elif key == 'discord':
                    self.discord.set(value)
                    if 'bağlandı' in value.lower() or 'connected' in value.lower():
                        self.status_pill.config(text='●  HAZIR', foreground=self.colors['accent'])
                elif key == 'preview':
                    self.preview.set(f"{value['details']}\n{value['state']}" if value else 'Aktif baskı bilgisi yok · Discord durumu gizli')
                elif key == 'error':
                    self.printer.set(value)
                    self.set_connection_state('error')
                elif key == 'stopped':
                    self.bridge = None
                    self.set_running(False)
                    self.printer.set('Bağlantı durduruldu')
                    self.discord.set('Discord durumu kapatıldı')
                    self.preview.set('Aktif bağlantı yok.')
                    self.set_connection_state('idle')
        except queue.Empty:
            pass
        if self.closing and self.bridge is None:
            if self.tray:
                self.tray.stop()
            self.root.destroy()
            return
        self.root.after(200, self.poll)

    def set_connection_state(self, state):
        labels = {
            'idle': ('●  BAĞLI DEĞİL', self.colors['muted']),
            'connected': ('●  YAZICI BAĞLI', self.colors['accent']),
            'warning': ('●  BAĞLANIYOR', '#f5c66b'),
            'error': ('●  BAĞLANTI HATASI', self.colors['danger']),
        }
        label, color = labels[state]
        self.status_pill.config(text=label, foreground=color)

    def hide(self):
        try:
            if self.tray is None:
                image = Image.open(self.resource_path('assets/app-icon-3d-printing-v1.png')).convert('RGBA').resize((64, 64), Image.Resampling.LANCZOS)
                self.tray = pystray.Icon('ThreeDPrinting', image, '3D Printing', pystray.Menu(
                    pystray.MenuItem('Ayarları aç', lambda *_: self.events.put(('show', None)), default=True),
                    pystray.MenuItem('Tamamen çık', lambda *_: self.events.put(('quit', None))),
                ))
                self.tray.run_detached()
            self.root.withdraw()
        except Exception:
            self.tray = None
            messagebox.showwarning('Arka plan', 'Bildirim alanı açılamadı. Pencere açık kalacak.')

    def quit(self):
        self.closing = True
        self.stop()


def main():
    # Prevent two copies from competing for the same Discord activity/settings.
    import ctypes
    ctypes.windll.kernel32.CreateMutexW.argtypes = [ctypes.c_void_p, ctypes.c_bool, ctypes.c_wchar_p]
    ctypes.windll.kernel32.CreateMutexW.restype = ctypes.c_void_p
    ctypes.windll.kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
    mutex = ctypes.windll.kernel32.CreateMutexW(None, False, 'Local\\BambuDiscordA1')
    if ctypes.windll.kernel32.GetLastError() == 183:
        ctypes.windll.user32.MessageBoxW(0, 'Uygulama zaten açık. Bildirim alanındaki simgeye bakın.', 'Bambu → Discord', 0)
        return
    try:
        application = App()
        if '--start' in sys.argv:
            application.root.after(400, application.start)
        application.root.mainloop()
    finally:
        ctypes.windll.kernel32.CloseHandle(mutex)


if __name__ == '__main__':
    main()
