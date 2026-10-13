#!/usr/bin/env python3
"""Ordon quality-approved name table validator and preview/export generator.

Reads the installed tables from content/kurovia/naming/ordon/ (or --content)
and writes the CSV exports next to this script. Spelling exceptions may list
result IDs ("results"/"name", as installed) or display names
("components"/"output", as supplied)."""
from pathlib import Path
from collections import Counter,defaultdict
import argparse,csv,json,random,itertools
ROOT=Path(__file__).resolve().parent
CONTENT=ROOT.parent.parent.parent/'content'/'kurovia'/'naming'/'ordon'

def rr(r):
    v=r['range'];return tuple(v) if isinstance(v,list) else (v,v)

def read():
    out={}
    for f in sorted(CONTENT.glob('[0-9][0-9]-*.json')):
        for t in json.loads(f.read_text(encoding='utf-8'))['tables']:
            assert t['id'] not in out,(f,t['id']);out[t['id']]=t
            d=int(t['dice'].split('d')[1]);faces=Counter()
            for r in t['results']:
                a,b=rr(r);assert 1<=a<=b<=d,(t['id'],a,b)
                faces.update(range(a,b+1))
            assert len(faces)==d and all(faces[k]==1 for k in range(1,d+1)),t['id']
            assert len({r['id'] for r in t['results']})==len(t['results'])
    schemes=json.loads((CONTENT/'naming-schemes.json').read_text(encoding='utf-8'))['naming_schemes']
    assert len(schemes)==1
    s=schemes[0];method=out[s['method_table']]
    assert {r['id'] for r in method['results']}=={m['result'] for m in s['methods']}
    for m in s['methods']:
        for t in m['components']:assert t in out,t
        for exc in m.get('exceptions',[]):
            key='results' if exc.get('results') else 'components'
            field='id' if key=='results' else 'name'
            assert len(exc[key])==len(m['components']),(m['result'],exc)
            for v,t in zip(exc[key],m['components']):
                assert sum(r[field]==v for r in out[t]['results'])==1,(m['result'],t,v)
        if m['join']['style']=='curated_variant':
            a,b=(out[k] for k in m['components'])
            for r in a['results']:
                assert set(r.get('approved_names',{}))=={v['id'] for v in b['results']},(m['result'],r['id'])
                assert all(str(x).strip() for x in r['approved_names'].values())
    return out,s

def combine(m,rs):
    for exc in m.get('exceptions',[]):
        if exc.get('results')==[r['id'] for r in rs] or exc.get('components')==[r['name'] for r in rs]:
            return exc.get('name',exc.get('output'))
    if m['join']['style']=='curated_variant':return rs[0]['approved_names'][rs[1]['id']].strip()
    if len(rs)==1:return rs[0]['name'].strip()
    if m['join']['style']=='compound':
        joined=''.join(r['name'].strip() for r in rs);return joined[:1].upper()+joined[1:].lower()
    if m['join']['style']=='space':return ' '.join(r['name'] for r in rs)
    raise ValueError(m['join'])

def roll(t,rng):
    n=rng.randint(1,int(t['dice'].split('d')[1]));return n,next(r for r in t['results'] if rr(r)[0]<=n<=rr(r)[1])

def export(tables,scheme,samples,seed):
    methods={r['result']:r for r in scheme['methods']};method_t=tables[scheme['method_table']]
    path=[];seen=defaultdict(list);sample=[]
    for mid,m in methods.items():
        cs=[tables[tid]['results'] for tid in m['components']]
        for results in itertools.product(*cs):
            name=combine(m,results)
            row={'method':mid,'name':name,'source':results[0]['name'],'variant':results[1]['name'] if len(results)>1 else ''}
            path.append(row);seen[name.casefold()].append(row)
    rng=random.Random(seed)
    for _ in range(samples):
        nr,mroll=roll(method_t,rng);mid=mroll['id'];m=methods[mid]
        rs=[roll(tables[t],rng) for t in m['components']]
        sample.append({'method':mid,'name':combine(m,[r for _,r in rs]),'method_roll':nr,'component_rolls':'; '.join(str(n) for n,_ in rs),'component_dice':'; '.join(tables[t]['dice'] for t in m['components'])})
    if samples:
        with (ROOT/'PREVIEW.csv').open('w',newline='',encoding='utf-8-sig') as f:
            w=csv.DictWriter(f,fieldnames=list(sample[0]));w.writeheader();w.writerows(sample)
    with (ROOT/'COMPLETE_CATALOG.csv').open('w',newline='',encoding='utf-8-sig') as f:
        w=csv.DictWriter(f,fieldnames=list(path[0]));w.writeheader();w.writerows(path)
    with (ROOT/'COLLISIONS.csv').open('w',newline='',encoding='utf-8-sig') as f:
        w=csv.DictWriter(f,fieldnames=['name','paths','methods']);w.writeheader()
        for lst in seen.values():
            if len(lst)>1:w.writerow({'name':lst[0]['name'],'paths':len(lst),'methods':'; '.join(sorted({r['method'] for r in lst}))})
    print(f'Validated {len(tables)} tables, {len(path)} paths, {len(seen)} spellings, {sum(len(x)>1 for x in seen.values())} duplicated spellings')
    print('Examples: '+', '.join(x['name'] for x in sample[:15]))

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--samples',type=int,default=20);p.add_argument('--seed',type=int,default=803);p.add_argument('--export',action='store_true');p.add_argument('--content',type=Path,default=CONTENT,help='folder holding the Ordon JSON tables');a=p.parse_args()
    CONTENT=a.content.resolve()
    tables,scheme=read()
    if a.export:export(tables,scheme,a.samples,a.seed)
    else:export(tables,scheme,a.samples,a.seed)
