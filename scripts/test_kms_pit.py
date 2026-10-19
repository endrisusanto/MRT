#!/usr/bin/env python3
"""
Test script to diagnose and inspect Samsung KMS REQ-PIT payload directly.
Usage on Windows (or any machine connected to Samsung Intranet):
    python test_kms_pit.py --kms https://kms.samsung.net --ad-token endri.s --cid 25112200 --nonce "+ENGMODES:0,OK,04:..."
"""

import sys
import json
import argparse
import urllib.request
import urllib.error
import ssl

def main():
    parser = argparse.ArgumentParser(description="Directly test Samsung KMS REQ-PIT endpoint")
    parser.add_argument("--kms", default="https://janus-kms-dev.sec.samsung.net", help="KMS base URL")
    parser.add_argument("--ad-token", default="endri.s", help="AD SSO Token / Username")
    parser.add_argument("--cid", default="25112200", help="8-char device CID")
    parser.add_argument("--ip", default="127.0.0.1", help="Client IP")
    parser.add_argument("--mac", default="00:00:00:00:00:00", help="Client MAC (XX:XX:XX:XX:XX:XX)")
    parser.add_argument("--user-type", default="", help="userType (empty string by default)")
    parser.add_argument("--nonce", help="Raw +ENGMODES:0,OK,04:... string from modem")
    
    args = parser.parse_args()

    # Sample dummy values for protocol 04 if nonce not provided
    esk_len = 512
    esk_b64_len = 684
    esk_b64 = "SAMPLE_ESK"
    erm_len = 273
    erm_b64_len = 364
    erm_b64 = "SAMPLE_ERM"
    hash_b64 = "SAMPLE_HASH"

    if args.nonce:
        parts = [p.strip() for p in args.nonce.split(':')]
        # Find 4 numbers representing esk_len, esk_b64_len, erm_len, erm_b64_len
        found = False
        for i in range(len(parts) - 6):
            if parts[i].isdigit() and parts[i+1].isdigit() and parts[i+3].isdigit() and parts[i+4].isdigit():
                esk_len = int(parts[i])
                esk_b64_len = int(parts[i+1])
                esk_b64 = parts[i+2]
                erm_len = int(parts[i+3])
                erm_b64_len = int(parts[i+4])
                erm_b64 = parts[i+5]
                hash_b64 = parts[i+6]
                found = True
                print(f"[Parsed Nonce] eskLen={esk_len}, ermLen={erm_len}, hash={hash_b64[:16]}...")
                break
        if not found:
            print("[Warning] Could not parse numeric fields from nonce, using raw values.")

    payload = {
        "msgType": "REQ-PIT",
        "protVer": "04",
        "userType": args.user_type,
        "auth": args.ad_token,
        "authLen": len(args.ad_token),
        "essId": "",
        "ip": args.ip,
        "mac": args.mac,
        "eeiLen": 0,
        "eeiB64Len": 0,
        "eeiB64": "",
        "eskLen": esk_len,
        "eskB64Len": eskB64_len,
        "eskB64": esk_b64,
        "ermLen": erm_len,
        "ermB64Len": erm_b64_len,
        "ermB64": erm_b64,
        "hashB64": hash_b64,
        "cid": args.cid
    }

    url = args.kms.rstrip('/')
    if not url.endswith('REQ-PIT'):
        url += '/REQ-PIT'

    data = json.dumps(payload, indent=2)
    print("=" * 60)
    print(f"Target URL: {url}")
    print("Sending Request Body:")
    print(data)
    print("=" * 60)

    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE

    req = urllib.request.Request(
        url,
        data=data.encode('utf-8'),
        headers={
            "Content-Type": "application/json",
            "Accept": "application/json",
            "User-Agent": "JanusServiceAgent/1.0"
        },
        method="POST"
    )

    try:
        with urllib.request.urlopen(req, context=ctx, timeout=15) as resp:
            print(f"HTTP Status: {resp.status} {resp.reason}")
            body = resp.read().decode('utf-8', errors='replace')
            print("Response Body:")
            try:
                parsed = json.loads(body)
                print(json.dumps(parsed, indent=2))
            except Exception:
                print(body)
    except urllib.error.HTTPError as e:
        print(f"HTTP Error: {e.code} {e.reason}")
        print("Response Body:")
        print(e.read().decode('utf-8', errors='replace'))
    except Exception as e:
        print(f"Connection Error: {e}")

if __name__ == "__main__":
    main()
