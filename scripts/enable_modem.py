#!/usr/bin/env python3
import sys
import subprocess
import time
import re
import xml.etree.ElementTree as ET

def get_center(bounds_str):
    m = re.findall(r'\d+', bounds_str)
    if len(m) == 4:
        return ((int(m[0]) + int(m[2])) // 2, (int(m[1]) + int(m[3])) // 2)
    return (0, 0)

def enable_modem_port(serial):
    print(f"[*] Auto-enabling modem port on Samsung device {serial}...")
    try:
        # 1. Wake screen
        subprocess.run(['adb', '-s', serial, 'shell', 'input keyevent 224'], timeout=3, check=False)
        time.sleep(0.2)
        
        # 2. Open dialer
        subprocess.run(['adb', '-s', serial, 'shell', 'am start -a android.intent.action.DIAL'], timeout=3, check=False)
        time.sleep(0.5)

        # 3. Clear existing digits
        for _ in range(15):
            subprocess.run(['adb', '-s', serial, 'shell', 'input keyevent 67'], timeout=1, check=False)
        time.sleep(0.2)

        # 4. Dump dialer UI to locate dialpad buttons dynamically
        subprocess.run(['adb', '-s', serial, 'shell', 'uiautomator dump /sdcard/ui.xml'], timeout=4, check=False)
        raw = subprocess.check_output(['adb', '-s', serial, 'shell', 'cat /sdcard/ui.xml'], timeout=3).decode(errors='ignore')
        
        root = ET.fromstring(raw)
        coords = {}
        for node in root.iter('node'):
            txt = node.attrib.get('text', '')
            desc = node.attrib.get('content-desc', '')
            bounds = node.attrib.get('bounds', '')
            if txt in ['*', '#', '0', '8']:
                coords[txt] = get_center(bounds)
            elif 'Pound' in desc and '#' not in coords:
                coords['#'] = get_center(bounds)
            elif 'Star' in desc and '*' not in coords:
                coords['*'] = get_center(bounds)

        if not all(k in coords for k in ['*', '#', '0', '8']):
            print(f"[-] Could not find dialpad buttons on {serial}")
            return False

        # 5. Tap *#0808#
        seq = ['*', '#', '0', '8', '0', '8', '#']
        for k in seq:
            x, y = coords[k]
            subprocess.run(['adb', '-s', serial, 'shell', f'input tap {x} {y}'], timeout=1, check=False)
            time.sleep(0.18)

        time.sleep(1.2)

        # 6. Dump USBSettings UI
        subprocess.run(['adb', '-s', serial, 'shell', 'uiautomator dump /sdcard/usb_ui.xml'], timeout=4, check=False)
        raw_usb = subprocess.check_output(['adb', '-s', serial, 'shell', 'cat /sdcard/usb_ui.xml'], timeout=3).decode(errors='ignore')
        
        root_usb = ET.fromstring(raw_usb)
        mtp_adb_coord = None
        ok_coord = None
        for node in root_usb.iter('node'):
            txt = node.attrib.get('text', '')
            res_id = node.attrib.get('resource-id', '')
            bounds = node.attrib.get('bounds', '')
            if 'mtp_adb_mode' in res_id or txt == 'MTP + ADB':
                mtp_adb_coord = get_center(bounds)
            if 'ok_button' in res_id or txt == 'OK':
                ok_coord = get_center(bounds)

        if mtp_adb_coord and ok_coord:
            # Tap MTP + ADB
            subprocess.run(['adb', '-s', serial, 'shell', f'input tap {mtp_adb_coord[0]} {mtp_adb_coord[1]}'], timeout=1, check=False)
            time.sleep(0.3)
            # Tap OK
            subprocess.run(['adb', '-s', serial, 'shell', f'input tap {ok_coord[0]} {ok_coord[1]}'], timeout=1, check=False)
            print(f"[+] Successfully triggered MTP+ADB mode on {serial}")
            return True
        else:
            print(f"[-] USBSettings buttons not found on {serial}")
            return False
    except Exception as e:
        print(f"[-] Error on {serial}: {e}")
        return False

if __name__ == '__main__':
    target_serial = sys.argv[1] if len(sys.argv) > 1 else None
    if not target_serial:
        print("Usage: enable_modem.py <SERIAL>")
        sys.exit(1)
    success = enable_modem_port(target_serial)
    sys.exit(0 if success else 1)
