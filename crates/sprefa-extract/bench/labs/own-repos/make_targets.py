import collections, csv, json, pathlib, re, subprocess, sys

R = pathlib.Path.cwd()
OUT = R / 'harness' / 'targets.tsv'
FIELDS = ['repo','topology','operation','file','name','at_byte','dest','dest_new','cross_package','collision_expected','allowed_files','module_parent','language','symbol_kind','check_package','timeout_seconds']
BAD_RXJS = {'rxjsx','json-rx','gothic','md','boop-xterm'}

QUERIES = {
 'hafley-rs': [
  ('fn','(function_item name: (identifier) @name)','rust','crates/sprefa-extract/src'),
  ('type','(struct_item name: (type_identifier) @name)','rust','crates/sprefa-extract/src'),
  ('type','(enum_item name: (type_identifier) @name)','rust','crates/sprefa-extract/src'),
  ('type','(type_item name: (type_identifier) @name)','rust','crates/sprefa-extract/src'),
  ('term','(const_item name: (identifier) @name)','rust','crates/sprefa-extract/src'),
  ('fn','(function_item name: (identifier) @name)','rust','crates/soopy/src'),
  ('type','(struct_item name: (type_identifier) @name)','rust','crates/soopy/src'),
  ('type','(enum_item name: (type_identifier) @name)','rust','crates/soopy/src'),
  ('type','(type_item name: (type_identifier) @name)','rust','crates/soopy/src'),
  ('term','(const_item name: (identifier) @name)','rust','crates/soopy/src'),
  ('fn','(function_item name: (identifier) @name)','rust','crates/ryi/src'),
  ('type','(struct_item name: (type_identifier) @name)','rust','crates/ryi/src'),
  ('type','(enum_item name: (type_identifier) @name)','rust','crates/ryi/src'),
  ('term','(const_item name: (identifier) @name)','rust','crates/ryi/src'),
 ],
 'hafley-rxjs': [
  ('fn','(function_declaration name: (identifier) @name)','ts','packages/signals/src'),
  ('type','(type_alias_declaration) @decl','ts','packages/signals/src'),
  ('type','(interface_declaration) @decl','ts','packages/signals/src'),
  ('type','(class_declaration name: (type_identifier) @name)','ts','packages/signals/src'),
  ('method','(method_definition name: (property_identifier) @name)','ts','packages/signals/src'),
 ],
 'hafley-tsp': [
  ('fn','(function_declaration name: (identifier) @name)','ts','packages'),
  ('type','(type_alias_declaration) @decl','ts','packages'),
  ('type','(interface_declaration) @decl','ts','packages'),
  ('type','(class_declaration name: (type_identifier) @name)','ts','packages'),
  ('method','(method_definition name: (property_identifier) @name)','ts','packages'),
 ],
}

def run(cmd):
    return subprocess.run(cmd, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)

def parse_declaration(text, kind):
    pattern = r'^(?:export\s+)?(?:declare\s+)?(?:default\s+)?(?:abstract\s+)?(?:type|interface|class)\s+([A-Za-z_$][\w$]*)\b'
    m = re.search(pattern, text.strip())
    return m.group(1) if m else None

def package_for(repo, rel):
    root = R/'repos'/repo
    p = root/rel
    for parent in [p.parent, *p.parents]:
        manifest = parent/'package.json'
        if manifest.exists():
            data = json.loads(manifest.read_text())
            if repo == 'hafley-rxjs':
                if parent.name in BAD_RXJS: return None
                if 'typecheck' not in data.get('scripts',{}): return None
                return data.get('name')
            if repo == 'hafley-tsp':
                if 'build' not in data.get('scripts',{}): return None
                return data.get('name')
        if parent == root: break
    return None

def candidates(repo):
    root = R/'repos'/repo
    found=[]
    for kind, query, lang, subdir in QUERIES[repo]:
        p=run(['ryi','query','--root',str(root),'--lang',lang,'--query',query,str(root/subdir)])
        for raw in p.stdout.splitlines():
            try: obj=json.loads(raw)
            except Exception: continue
            name=obj.get('name') or parse_declaration(obj.get('decl',''),kind)
            rel=pathlib.Path(obj['path']).relative_to(root).as_posix()
            if not name or any(x in rel.split('/') for x in ('tests','test','__tests__')) or '.test.' in rel or '.spec.' in rel: continue
            if repo != 'hafley-rs':
                pkg=package_for(repo,rel)
                if not pkg: continue
            else: pkg='sprefa-extract'
            data=(root/rel).read_bytes().splitlines(keepends=True)
            line=obj['line']-1
            if line < 0 or line >= len(data): continue
            m=re.search(rb'(?<![A-Za-z0-9_$])'+re.escape(name.encode())+rb'(?![A-Za-z0-9_$])',data[line])
            if not m:
                # Multiline declarations still carry the name on a nearby syntax line.
                continue
            byte=sum(map(len,data[:line]))+m.start()
            found.append({'kind':kind,'name':name,'file':rel,'at_byte':byte,'package':pkg,'language':lang})
    seen=set(); unique=[]
    for x in found:
        k=(x['file'],x['name'],x['at_byte'])
        if k not in seen: seen.add(k); unique.append(x)
    return unique

def graph_files(repo, c):
    root=R/'repos'/repo
    if c['kind'] in ('fn','method'):
        flag='--callers'
    else: flag='--uses'
    p=run(['ryi','graph',flag,c['name'],'--root',str(root),str(root/pathlib.Path(c['file']).parent)])
    files={c['file']}
    for raw in p.stdout.splitlines():
        try: obj=json.loads(raw)
        except Exception: continue
        for key in ('from_path','to_path'):
            value=obj.get(key)
            if value:
                q=pathlib.Path(value)
                try: files.add(q.relative_to(root).as_posix())
                except ValueError: pass
    return files

def parse_candidate_declarations(repo, directory):
    root=R/'repos'/repo
    out=[]
    for kind, query, lang, subdir in QUERIES[repo]:
        p=run(['ryi','query','--root',str(root),'--lang',lang,'--query',query,str(root/subdir)])
        for raw in p.stdout.splitlines():
            try: obj=json.loads(raw)
            except Exception: continue
            name=obj.get('name') or parse_declaration(obj.get('decl',''),kind)
            if not name: continue
            rel=pathlib.Path(obj['path']).relative_to(root).as_posix()
            if pathlib.Path(rel).parent.as_posix()!=directory: continue
            if any(x in rel.split('/') for x in ('tests','test','__tests__')) or '.test.' in rel or '.spec.' in rel: continue
            if repo!='hafley-rs' and not package_for(repo,rel): continue
            line=obj['line']-1; data=(root/rel).read_bytes().splitlines(keepends=True)
            if line>=len(data): continue
            m=re.search(rb'(?<![A-Za-z0-9_$])'+re.escape(name.encode())+rb'(?![A-Za-z0-9_$])',data[line])
            if not m: continue
            out.append({'kind':kind,'name':name,'file':rel,'at_byte':sum(map(len,data[:line]))+m.start(),'package':package_for(repo,rel) if repo!='hafley-rs' else 'sprefa-extract','language':lang})
    return out

def destination_options(repo, source, existing):
    root=R/'repos'/repo
    source_path=pathlib.Path(source['file'])
    if repo=='hafley-rs':
        folder=source_path.parent
        siblings=[p for p in (root/folder).glob('*.rs') if p.name not in {'mod.rs','lib.rs'} and p.as_posix()!=str(root/source['file'])]
        new=(folder/f'moved_{source_path.stem}_{source["name"]}.rs').as_posix()
        parent=folder/'mod.rs' if (root/folder/'mod.rs').exists() else (folder.parent/(folder.name+'.rs'))
        if folder==pathlib.Path('crates/sprefa-extract/src'): parent=folder/'lib.rs'
        parent_rel=parent.as_posix() if (root/parent).exists() else ''
        candidates=[p.relative_to(root).as_posix() for p in siblings]
        return new,candidates,parent_rel
    folder=source_path.parent
    new=(folder/f'moved_{source_path.stem}_{source["name"]}.ts').as_posix()
    siblings=[p for p in (root/folder).glob('*.ts*') if p.as_posix()!=str(root/source['file']) and '.test.' not in p.name and '.spec.' not in p.name]
    return new,[p.relative_to(root).as_posix() for p in siblings],''

def has_name(repo, path, name):
    return bool(re.search(rb'(?<![A-Za-z0-9_$])'+re.escape(name.encode())+rb'(?![A-Za-z0-9_$])',(R/'repos'/repo/path).read_bytes()))

def main():
    result=[]
    for repo in ('hafley-rs','hafley-rxjs','hafley-tsp'):
        allc=candidates(repo)
        # Graph selects the 20 cross-file rename rows. Keep one semantic result per declaration.
        ext=[]; local=[]
        kinds=('fn','type','term') if repo=='hafley-rs' else ('fn','type','method')
        graph_candidates=[]
        # Bound graph discovery while retaining each queried declaration category.
        for kind in kinds:
            by_crate=collections.defaultdict(list)
            for c in allc:
                if c['kind']==kind:
                    crate='/'.join(pathlib.Path(c['file']).parts[:2])
                    by_crate[crate].append(c)
            for crate in sorted(by_crate): graph_candidates.extend(by_crate[crate][:35])
        for c in graph_candidates:
            allowed=graph_files(repo,c)
            c['allowed_files']=allowed
            if any(f!=c['file'] for f in allowed): ext.append(c)
            else: local.append(c)
        if len(ext)<20 or len(local)<20:
            raise SystemExit(f'{repo}: graph returned {len(ext)} cross-file and {len(local)} local rename targets')
        def pick_kinds(pool, quotas):
            chosen=[]
            for kind, count in quotas:
                for c in pool:
                    if len([x for x in chosen if x['kind']==kind])>=count: break
                    if c['kind']==kind and c not in chosen: chosen.append(c)
            for c in pool:
                if len(chosen)>=20: break
                if c not in chosen: chosen.append(c)
            return chosen[:20]
        quota=[('fn',10),('type',5),('term',5)] if repo=='hafley-rs' else [('fn',10),('type',5),('method',5)]
        selected=pick_kinds(ext,quota)+pick_kinds(local,quota)
        for c in selected:
            result.append({'repo':repo,'topology':'rust_workspace' if repo=='hafley-rs' else 'monorepo','operation':'rename','file':c['file'],'name':c['name'],'at_byte':c['at_byte'],'dest':'','dest_new':'False','cross_package':'False','collision_expected':'False','allowed_files':';'.join(sorted(c['allowed_files'])),'module_parent':'','language':c['language'],'symbol_kind':c['kind'],'check_package':c['package'],'timeout_seconds':{'hafley-rs':957,'hafley-rxjs':17,'hafley-tsp':28}[repo]})
        # Cleaves: 10 valid new files, 5 existing same-package files, 5 cross-package refusals.
        made_new=made_existing=made_cross=0
        for c in allc:
            if c['kind'] not in ('fn','method'): continue
            if any(x['name']==c['name'] and x['file']==c['file'] for x in selected): pass
            new, siblings, module_parent=destination_options(repo,c,allc)
            if made_new<10:
                dest=new; isnew=True; cross=False
            elif made_existing<5:
                opts=[p for p in siblings if not has_name(repo,p,c['name'])]
                if not opts: continue
                dest=opts[0]; isnew=False; cross=False
            elif made_cross<5 and repo!='hafley-rs':
                other=[]
                for path in sorted((root/'packages').rglob('*.ts')):
                    rel=path.relative_to(root).as_posix()
                    dest_pkg=package_for(repo,rel)
                    if dest_pkg and dest_pkg!=c['package'] and '.test.' not in rel and '.spec.' not in rel:
                        other.append({'file':rel,'package':dest_pkg})
                opts=[]
                for x in other:
                    if has_name(repo,x['file'],c['name']): continue
                    # Existing file destination in another package, selected through ryi declarations.
                    opts.append(x['file'])
                if not opts: continue
                dest=opts[0]; isnew=False; cross=True
            elif made_cross<5 and repo=='hafley-rs':
                others=[p for p in (R/'repos'/repo/'crates').glob('*/src/lib.rs') if p.as_posix()!=str(R/'repos'/repo/c['file'])]
                opts=[p.relative_to(R/'repos'/repo).as_posix() for p in others if not has_name(repo,p.relative_to(R/'repos'/repo).as_posix(),c['name'])]
                if not opts: continue
                dest=opts[0]; isnew=False; cross=True
            else: break
            source_files=graph_files(repo,c)
            root=R/'repos'/repo
            if cross and repo!='hafley-rs':
                # Cross-package target is intentionally directed to a package that does not declare a dependency.
                dest_pkg=package_for(repo,dest)
                source_manifest=json.loads((root/pathlib.Path(c['file']).parents[1]/'package.json').read_text()) if (root/pathlib.Path(c['file']).parents[1]/'package.json').exists() else {}
                deps={**source_manifest.get('dependencies',{}),**source_manifest.get('devDependencies',{})}
                if dest_pkg in deps: continue
            if made_new<10 and isnew: made_new+=1
            elif cross: made_cross+=1
            else: made_existing+=1
            allowed=set(source_files)|{c['file'],dest}
            result.append({'repo':repo,'topology':'rust_workspace' if repo=='hafley-rs' else 'monorepo','operation':'cleave','file':c['file'],'name':c['name'],'at_byte':c['at_byte'],'dest':dest,'dest_new':str(isnew),'cross_package':str(cross),'collision_expected':'False','allowed_files':';'.join(sorted(allowed)),'module_parent':module_parent if isnew else '','language':c['language'],'symbol_kind':c['kind'],'check_package':c['package'],'timeout_seconds':{'hafley-rs':957,'hafley-rxjs':17,'hafley-tsp':28}[repo]})
            if (made_new,made_existing,made_cross)==(10,5,5): break
        if (made_new,made_existing,made_cross)!=(10,5,5):
            raise SystemExit(f'{repo}: cleave composition {made_new} new, {made_existing} existing, {made_cross} cross-package')
    with OUT.open('w',newline='') as f:
        w=csv.DictWriter(f,fieldnames=FIELDS,delimiter='\t'); w.writeheader(); w.writerows(result)
    print(f'wrote {len(result)} rows to {OUT}')

if __name__=='__main__': main()
