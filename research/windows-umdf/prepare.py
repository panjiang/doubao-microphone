"""Prepare the UMDF P0 package; sign with a disposable file-only test key.
Does not install anything. Private signing material is deleted after use.
"""
from pathlib import Path
import subprocess, shutil, tempfile, os, secrets, hashlib, json

source = Path(__file__).resolve().parent
build = Path('/mnt/c/Users/Administrator/AppData/Local/Temp/doubao-ptt-umdf-research')
ledger = build / 'out/installation-ledger.json'
if ledger.exists() and json.loads(ledger.read_text(encoding='utf-8-sig')).get('phase') != 'removed':
    raise SystemExit('An installed research package exists. Preserve it and its certificate for rollback; do not overwrite it.')
package = build/'out/package'
package.mkdir(exist_ok=True)
def run(command, **kwargs):
    result = subprocess.run(command, capture_output=True, text=True, **kwargs)
    if result.returncode:
        raise SystemExit(result.stdout + result.stderr)
    return result.stdout
def win(path): return run(['wslpath','-w',str(path)]).strip()

for name in ['DoubaoPTTUm.dll','DoubaoPTTUm.inf','umdf-probe.exe']:
    shutil.copy2(build/'out'/name, package/name)
signtool='/mnt/c/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/signtool.exe'
inf2cat='/mnt/c/Users/Administrator/AppData/Local/Temp/doubao-ptt-vhf-research/out/toolchain/wdk/c/bin/10.0.26100.0/x86/Inf2Cat.exe'
pfx=build/'out/transient-umdf.pfx'
with tempfile.TemporaryDirectory(prefix='ptt-umdf-sign-') as temporary:
    folder=Path(temporary);key=folder/'key.pem';cert=folder/'cert.pem'
    password=secrets.token_urlsafe(32)
    environment={**os.environ,'PTT_UMDF_SIGN_PASSWORD':password}
    run(['openssl','req','-new','-x509','-newkey','rsa:3072','-sha256','-days','14',
         '-subj','/CN=DoubaoPTT UMDF Research',
         '-addext','basicConstraints=critical,CA:FALSE',
         '-addext','keyUsage=critical,digitalSignature',
         '-addext','extendedKeyUsage=codeSigning',
         '-keyout',str(key),'-out',str(cert),'-passout','env:PTT_UMDF_SIGN_PASSWORD'],env=environment)
    public=package/'DoubaoPTTUm.cer'
    run(['openssl','x509','-in',str(cert),'-outform','DER','-out',str(public)])
    try:
        run(['openssl','pkcs12','-export','-inkey',str(key),'-in',str(cert),'-out',str(pfx),
             '-passin','env:PTT_UMDF_SIGN_PASSWORD','-passout','env:PTT_UMDF_SIGN_PASSWORD'],env=environment)
        for name in ['DoubaoPTTUm.dll','umdf-probe.exe']:
            run([signtool,'sign','/fd','SHA256','/f',win(pfx),'/p',password,win(package/name)])
        print(run([inf2cat,'/driver:'+win(package),'/os:10_CO_X64,10_NI_X64,10_GE_X64']))
        run([signtool,'sign','/fd','SHA256','/f',win(pfx),'/p',password,win(package/'DoubaoPTTUm.cat')])
    finally:
        pfx.unlink(missing_ok=True)
manifest={'schema':1,'hardware_id':'ROOT\\DOUBAOPTT_UMDF_RESEARCH',
          'certificate_thumbprint':hashlib.sha1(public.read_bytes()).hexdigest().upper(),
          'private_key_deleted':True,'sha256':{}}
for path in sorted(package.iterdir()):
    if path.is_file() and path.name!='manifest.json':
        manifest['sha256'][path.name]=hashlib.sha256(path.read_bytes()).hexdigest()
(package/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'package':str(package),'thumbprint':manifest['certificate_thumbprint'],
                  'private_key_deleted':True,'system_changes':False},indent=2))
