#!/usr/bin/env python3
"""Compare mirror lockfile checksums with official registries (metadata only).
No packages are downloaded or executed. Cache results in ignored artifacts/.
"""
import concurrent.futures, hashlib, json, pathlib, re, sys, urllib.request
ROOT = pathlib.Path(__file__).resolve().parents[1]
CACHE = ROOT / 'artifacts' / 'registry-verification'
CACHE.mkdir(parents=True, exist_ok=True)

def read(url):
    key = hashlib.sha256(url.encode()).hexdigest()
    path = CACHE / key
    if not path.exists():
        req = urllib.request.Request(url, headers={'User-Agent':'doubao-ptt-dependency-check/1'})
        data = urllib.request.urlopen(req, timeout=30).read()
        path.write_bytes(data)
    return path.read_bytes()

def cargo(p):
    n = p['name']
    route = n if len(n)==1 else '2/'+n if len(n)==2 else '3/'+n[0]+'/'+n if len(n)==3 else n[:2]+'/'+n[2:4]+'/'+n
    records = [json.loads(line) for line in read('https://index.crates.io/'+route).splitlines()]
    official = next(x for x in records if x['vers']==p['version'])
    assert official['cksum']==p['checksum'], f"Cargo checksum mismatch: {n}@{p['version']}"

def npm(item):
    path,p = item
    name = path.split('node_modules/')[-1]
    official=json.loads(read('https://registry.npmjs.org/'+name+'/'+p['version']))
    assert official['dist']['integrity']==p['integrity'], f"npm integrity mismatch: {name}@{p['version']}"

jobs = []
for block in (ROOT/'Cargo.lock').read_text().split('[[package]]')[1:]:
    p={key:json.loads(value) for key,value in re.findall(r'^(name|version|checksum) = ("[^"\n]*")$',block,re.M)}
    if 'checksum' in p: jobs.append((cargo,p))
lock=json.loads((ROOT/'web/package-lock.json').read_text())
jobs += [(npm,(path,p)) for path,p in lock['packages'].items() if path and 'integrity' in p]
errors=[]
def run(job):
    fn,arg=job
    try: fn(arg)
    except Exception as e: return str(e)
with concurrent.futures.ThreadPoolExecutor(max_workers=12) as executor:
    errors=[e for e in executor.map(run,jobs) if e]
report={'checked':len(jobs),'ok':not errors,'errors':errors}
(ROOT/'artifacts/dependency-verification.json').write_text(json.dumps(report,indent=2))
print(json.dumps(report,ensure_ascii=False,indent=2))
sys.exit(bool(errors))
