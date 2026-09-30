import csv, json, pathlib, sqlite3, subprocess, sys, time, os, shutil, signal

R = pathlib.Path.cwd()
binary = pathlib.Path(sys.argv[1]).resolve()
mode = sys.argv[2]
time_limit = int(os.environ.get('CORPUS_TARGET_TIMEOUT', '120'))
if mode not in {'fast', 'slow', 'oracle'}:
    raise SystemExit('mode must be fast, slow, or oracle')
# The ryi client uses one daemon socket below XDG_CACHE_HOME. Give each run
# its own cache so concurrent worktrees and binary builds cannot conflict.
run_tag = os.environ.get('CORPUS_RUN_TAG', 'default')
os.environ['XDG_CACHE_HOME'] = str(R/'build'/f'ryi-cache-{mode}-{run_tag}')
rows = list(csv.DictReader(open(os.environ.get('CORPUS_TARGETS_FILE',str(R/'targets.tsv'))), delimiter='\t'))
selected_repos = set(filter(None, os.environ.get('CORPUS_REPOS', '').split(',')))
if selected_repos:
    rows = [row for row in rows if row['repo'] in selected_repos]
db = sqlite3.connect(R/'results.db')
db.execute('''CREATE TABLE IF NOT EXISTS corpus_score (
 repo TEXT, topology TEXT, operation TEXT, tool TEXT, mode TEXT, target TEXT,
 pass INTEGER, timed_out INTEGER, status TEXT, files_touched INTEGER,
 allowed_files INTEGER, seconds REAL, detail TEXT, target_timeout_seconds INTEGER,
 PRIMARY KEY (repo, operation, tool, mode, target))''')
if 'target_timeout_seconds' not in [r[1] for r in db.execute('PRAGMA table_info(corpus_score)')]:
    db.execute('ALTER TABLE corpus_score ADD COLUMN target_timeout_seconds INTEGER')
db.commit()

def check_command(row):
    if row['repo'] == 'hafley-rs':
        if row['file'].startswith('crates/sprefa-extract/'):
            return ['cargo','check','--features','cli','--locked','-j','2']
        return ['cargo','check','--workspace','--all-targets','--locked','-j','2']
    if row['repo'] == 'hafley-rxjs':
        return ['pnpm','--filter',row['check_package'],'typecheck']
    if row['repo'] == 'hafley-tsp':
        return ['pnpm','--filter',row['check_package'],'build']
    raise ValueError(f"missing check command for {row['repo']}")

def check_root(row, root):
    if row['repo'] == 'hafley-rs' and row['file'].startswith('crates/sprefa-extract/'):
        return root/'crates'/'sprefa-extract'
    return root

def call(cmd, cwd, timeout):
    p=subprocess.Popen(cmd, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                       env=os.environ.copy(), start_new_session=True)
    try:
        out,_=p.communicate(timeout=max(0.01,timeout))
        return subprocess.CompletedProcess(cmd,p.returncode,out)
    except subprocess.TimeoutExpired as e:
        try: os.killpg(p.pid,signal.SIGKILL)
        except ProcessLookupError: pass
        out,_=p.communicate()
        raise subprocess.TimeoutExpired(cmd,timeout,output=out)

def repo_root(repo):
    override=os.environ.get('CORPUS_ROOT_OVERRIDE')
    return pathlib.Path(override).resolve() if override else R/'repos'/repo

def oracle_path(row):
    repo, op = row['repo'], row['operation']
    lane = 'ts' if repo in {'ajv','codegraph-src','vite','hafley-rxjs','hafley-tsp'} else 'rust'
    stem = f"{repo}__{op}__{row['file'].replace('/','_')}__{row['name']}__{row['at_byte']}.json"
    return R/'oracles'/lane/stem

def oracle_files(row):
    files = {row['file']}
    if row['dest']: files.add(row['dest'])
    files.update(x for x in row.get('allowed_files','').split(';') if x)
    if row.get('module_parent'): files.add(row['module_parent'])
    p = oracle_path(row)
    if not p.exists(): return files
    try:
        d = json.load(open(p))
        for edit in d.get('edits', []):
            if edit.get('file'): files.add(edit['file'])
            for change in edit.get('textChanges', []):
                pass
        res = d.get('response', {}).get('result') or {}
        for uri in (res.get('changes') or {}):
            if uri.startswith('file://'):
                f=pathlib.Path(uri[7:])
                try: rel=f.relative_to(R/'repos'/row['repo']).as_posix()
                except ValueError:
                    try: rel=f.relative_to(R/'repos'/f"{row['repo']}-competitor").as_posix()
                    except ValueError: continue
                files.add(rel)
        for dc in res.get('documentChanges') or []:
            uri = dc.get('textDocument', {}).get('uri', '')
            if uri.startswith('file://'):
                f=pathlib.Path(uri[7:])
                try: rel=f.relative_to(R/'repos'/row['repo']).as_posix()
                except ValueError:
                    try: rel=f.relative_to(R/'repos'/f"{row['repo']}-competitor").as_posix()
                    except ValueError: continue
                files.add(rel)
    except Exception: pass
    return files

def apply_oracle(row):
    repo, op, root = row['repo'], row['operation'], R/'repos'/row['repo']
    p = oracle_path(row)
    if not p.exists(): return False, set(), 'oracle JSON missing'
    d = json.load(open(p)); changes = {}
    if repo in {'ajv','codegraph-src','vite','hafley-rxjs','hafley-tsp'}:
        if op == 'rename':
            if not d.get('renameInfo', {}).get('canRename'): return False, set(), d.get('renameInfo', {}).get('localizedErrorMessage','rename unavailable')
            for loc in d.get('locations') or []:
                f = pathlib.Path(loc['fileName'])
                try: rel = f.relative_to(root).as_posix()
                except ValueError: continue
                span = loc['textSpan']; changes.setdefault(rel, []).append((span['start'], span['length'], row['name']+'_oracle_renamed'))
        else:
            if not d.get('edits'): return False, set(), d.get('error') or 'no Move to file edits returned'
            for e in d['edits']:
                for ch in e.get('textChanges', []): changes.setdefault(e['file'], []).append((ch['span']['start'], ch['span']['length'], ch['newText']))
    else:
        if op == 'cleave': return False, set(), 'Rust oracle rule is none for cleave'
        res = (d.get('response') or {}).get('result')
        if not res: return False, set(), d.get('error') or 'rename response absent'
        edit_map = dict(res.get('changes') or {})
        for dc in res.get('documentChanges') or []:
            edit_map.setdefault(dc.get('textDocument', {}).get('uri',''), dc.get('edits', []))
        for uri, edits in edit_map.items():
            if not uri.startswith('file://'): continue
            f = pathlib.Path(uri[7:])
            try: rel = f.relative_to(root).as_posix()
            except ValueError:
                try: rel = f.relative_to(R/'repos'/f"{repo}-competitor").as_posix()
                except ValueError: continue
            content = (root/rel).read_text()
            def offset(pos):
                lines = content.splitlines(keepends=True)
                prefix = ''.join(lines[:pos['line']])
                body = lines[pos['line']] if pos['line'] < len(lines) else ''
                units = pos['character']; utf16 = body.encode('utf-16-le')[:units*2].decode('utf-16-le', errors='ignore')
                return len((prefix+utf16).encode())
            for e in edits:
                try: start, end = offset(e['range']['start']), offset(e['range']['end'])
                except Exception: continue
                changes.setdefault(rel, []).append((start, end-start, e['newText']))
    if not changes: return False, set(), d.get('error') or 'oracle returned no in-repository edits'
    for rel, edits in changes.items():
        f = root/rel; original = f.read_text() if f.exists() else ''; b = original.encode()
        def byte_offset(utf16_offset):
            units=0; size=0
            for ch in original:
                if units >= utf16_offset: break
                units += len(ch.encode('utf-16-le'))//2; size += len(ch.encode())
            return size
        byte_edits=[]
        for start,length,replacement in edits:
            if repo in {'ajv','codegraph-src','vite','hafley-rxjs','hafley-tsp'}:
                bstart=byte_offset(start); bend=byte_offset(start+length)
                byte_edits.append((bstart,bend-bstart,replacement))
            else: byte_edits.append((start,length,replacement))
        for start, length, replacement in sorted(byte_edits, key=lambda x:x[0], reverse=True): b = b[:start]+replacement.encode()+b[start+length:]
        f.parent.mkdir(parents=True, exist_ok=True); f.write_bytes(b)
    return True, set(changes), ''

def changed_paths(root):
    out = subprocess.run(['git','-C',str(root),'status','--porcelain','--untracked-files=all'],text=True,stdout=subprocess.PIPE).stdout.splitlines()
    paths = set()
    for line in out:
        rel = line[3:].split(' -> ')[-1]
        if rel and not any(x in rel.split('/') for x in ('.git','node_modules','build','.cargo-home')): paths.add(rel)
    return paths

def restore(root, touched):
    subprocess.run(['git','-C',str(root),'reset','--hard','HEAD'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    for rel in touched:
        p = root/rel
        tracked = subprocess.run(['git','-C',str(root),'ls-files','--error-unmatch',rel],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode == 0
        if tracked: continue
        if p.is_dir(): shutil.rmtree(p)
        elif p.exists(): p.unlink()

def save(row, tool, ok, timed_out, status, touched, allowed, seconds, detail, target_limit):
    detail_text = str(detail)
    if len(detail_text) > 2000:
        detail_text = detail_text[:1300] + '\n[detail clipped]\n' + detail_text[-650:]
    db.execute('INSERT OR REPLACE INTO corpus_score (repo,topology,operation,tool,mode,target,pass,timed_out,status,files_touched,allowed_files,seconds,detail,target_timeout_seconds) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
      (row['repo'],row['topology'],row['operation'],tool,mode,
       f"{row['file']}#{row['name']}@{row['at_byte']}",
       None if ok is None else int(ok),int(timed_out),status,len(touched),len(allowed),round(seconds,3),detail_text,target_limit))
    db.commit()

for i,row in enumerate(rows,1):
    repo, root = row['repo'], repo_root(row['repo'])
    target_limit = int(row.get('timeout_seconds') or os.environ.get(f'CORPUS_TARGET_TIMEOUT_{repo}', time_limit))
    tool = ('typescript-5.9.3' if row.get('language') == 'ts' else 'rust-analyzer') if mode=='oracle' else 'ryii'
    key = f"{row['file']}#{row['name']}@{row['at_byte']}"
    exists = db.execute('SELECT 1 FROM corpus_score WHERE repo=? AND operation=? AND tool=? AND mode=? AND target=?',
                         (repo,row['operation'],tool,mode,key)).fetchone()
    if exists:
        continue
    started = time.monotonic(); deadline = started+target_limit; allowed = oracle_files(row); touched=set(); before=changed_paths(root); timed_out=False; detail=''; tool_ok=False; typecheck_ok=False
    if repo == 'hafley-rs':
        os.environ['CARGO_HOME'] = os.environ.get('CORPUS_CARGO_HOME',str(root/'.cargo-home'))
        os.environ['CARGO_TARGET_DIR'] = os.environ.get('CORPUS_CARGO_TARGET_DIR',str(root/'build'/'rs-target'))
    if mode == 'oracle' and repo in {'anyhow','tokio','hafley-rs'} and row['operation']=='cleave':
        save(row,tool,False,False,'oracle_unavailable',touched,allowed,time.monotonic()-started,'Rust oracle records none for cleave',target_limit)
        print(json.dumps({'i':i,'total':len(rows),'tool':tool,'target':key,'status':'oracle_unavailable'}),flush=True); continue
    try:
        if mode == 'oracle':
            tool_ok,touched,detail = apply_oracle(row)
        else:
            args=[str(binary)]
            if row['operation']=='rename': args += ['rename',f"{row['file']}#{row['name']}",row['name']+'_codex_renamed','--at',row['at_byte']]
            else: args += ['cleave',f"{row['file']}#{row['name']}",row['dest']]
            args += ['--root',str(root),'--state',str(R/'build'/f"soopy-state-{repo}-{mode}-{run_tag}"),'--commit']
            if mode=='slow': args += ['--slow']
            p=call(args,root,max(0.01,deadline-time.monotonic())); tool_ok=p.returncode==0; detail=(detail+'; ' if detail and p.stdout else '')+p.stdout[-1500:]
        touched |= (changed_paths(root)-before)
        if tool_ok:
            check=call(check_command(row),check_root(row,root),max(0.01,deadline-time.monotonic()))
            typecheck_ok=check.returncode==0 and not any(line.lstrip().startswith(('error:','error[')) or ': error TS' in line for line in check.stdout.splitlines())
            if not typecheck_ok:
                error_lines=[line.strip() for line in check.stdout.splitlines() if line.lstrip().startswith(('error:','error[')) or ': error TS' in line]
                detail += '\nTYPECHECK: '+check.stdout[-1500:]
                if error_lines: detail += '\nTYPECHECK_ERRORS: '+' | '.join(error_lines[:6])
    except subprocess.TimeoutExpired as e:
        touched |= changed_paths(root)-before
        timed_out=True
        captured=e.output.decode(errors='replace') if isinstance(e.output,bytes) else (e.output or '')
        detail += f' per-target timeout after {target_limit}s\n{captured[-1200:]}'
    except Exception as e:
        touched |= changed_paths(root)-before
        detail += f' {type(e).__name__}: {e}'
    timed_out = timed_out or time.monotonic()>=deadline
    reason = str(detail).lower()
    refused = (not tool_ok and any(marker in reason for marker in ('must depend on', 'abstain', 'cannot ', 'collision', 'refus', 'not supported', 'glob import reaches the symbol at runtime')))
    passed = tool_ok and typecheck_ok and touched.issubset(allowed)
    if timed_out: status='timeout'
    elif refused: status='correct_refusal'
    elif not tool_ok: status='tool_failed'
    elif not typecheck_ok: status='typecheck_failed'
    elif not touched.issubset(allowed): status='unexpected_files'
    else: status='pass'
    restore(root,touched)
    elapsed=time.monotonic()-started
    save(row,tool,passed,timed_out,status,touched,allowed,elapsed,detail,target_limit)
    print(json.dumps({'i':i,'total':len(rows),'tool':tool,'target':key,'status':status,'seconds':round(elapsed,2)}),flush=True)
db.close()
