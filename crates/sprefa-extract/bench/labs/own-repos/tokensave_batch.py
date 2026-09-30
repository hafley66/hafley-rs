import asyncio, csv, json, os, pathlib, re, sqlite3, subprocess, sys, time
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

R=pathlib.Path.cwd(); BIN=pathlib.Path.home()/'.cache/lanes/claude-375/eval/tokensave/bin/tokensave'
LAB=R/'harness'/'scratch'/'competitors'/'tokensave'
RUSTUP_HOME=pathlib.Path.home()/'.rustup'
ROWS=list(csv.DictReader(open(os.environ.get('TOKEN_SAVE_TARGETS_FILE',R/'targets.tsv')),delimiter='\t'))
DB=sqlite3.connect(R/'results.db')
DB.execute('''CREATE TABLE IF NOT EXISTS corpus_score (
 repo TEXT, topology TEXT, operation TEXT, tool TEXT, mode TEXT, target TEXT,
 pass INTEGER, timed_out INTEGER, status TEXT, files_touched INTEGER,
 allowed_files INTEGER, seconds REAL, detail TEXT, target_timeout_seconds INTEGER,
 PRIMARY KEY (repo, operation, tool, mode, target))''')
if 'target_timeout_seconds' not in [x[1] for x in DB.execute('PRAGMA table_info(corpus_score)')]:
    DB.execute('ALTER TABLE corpus_score ADD COLUMN target_timeout_seconds INTEGER')
DB.commit()

def call(cmd,cwd,env,timeout):
    p=subprocess.run(cmd,cwd=cwd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=timeout)
    return p.returncode,p.stdout

def json_text(result):
    for block in result.content:
        value=getattr(block,'text',None)
        if value:
            try:
                starts=[i for i in (value.find('['),value.find('{')) if i>=0]
                if not starts: continue
                parsed,_=json.JSONDecoder().raw_decode(value,min(starts))
                for _ in range(4):
                    if isinstance(parsed,dict) and isinstance(parsed.get('content'),list):
                        nested=next((x.get('text') for x in parsed['content'] if isinstance(x,dict) and isinstance(x.get('text'),str)),None)
                        if nested is None: break
                        try: parsed=json.loads(nested)
                        except Exception: break
                    else: break
                return parsed
            except Exception: continue
    return None

def touched_paths(root):
    p=subprocess.run(['git','status','--porcelain','--untracked-files=all'],cwd=root,text=True,stdout=subprocess.PIPE)
    return {line[3:].split(' -> ')[-1] for line in p.stdout.splitlines() if line[3:] and not any(x in line[3:].split('/') for x in ('.tokensave','.git','.serena','.cargo-home','build','node_modules'))}

def restore(root, touched):
    subprocess.run(['git','reset','--hard','HEAD'],cwd=root,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    for rel in touched:
        p=root/rel
        if subprocess.run(['git','ls-files','--error-unmatch',rel],cwd=root,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==0: continue
        if p.is_dir():
            import shutil; shutil.rmtree(p)
        elif p.exists(): p.unlink()

def check_command(row):
    if row['repo']=='hafley-rs' and row['file'].startswith('crates/sprefa-extract/'): return ['cargo','check','--features','cli','--locked','-j','2']
    if row['repo']=='hafley-rs': return ['cargo','check','--workspace','--all-targets','--locked','-j','2']
    if row['repo']=='hafley-rxjs': return ['pnpm','--filter',row['check_package'],'typecheck']
    return ['pnpm','--filter',row['check_package'],'build']

def check_root(row,root):
    if row['repo']=='hafley-rs' and row['file'].startswith('crates/sprefa-extract/'): return root/'crates'/'sprefa-extract'
    return root

def env_for(root):
    env=os.environ.copy(); env['HOME']=str(LAB/'tokensave-home'); env['TOKENSAVE_DISABLE_GREP_HOOK']='1'
    if 'hafley-rs' in root.name:
        env['CARGO_HOME']=str(R/'repos/hafley-rs/.cargo-home'); env['CARGO_TARGET_DIR']=str(root/'build'/'rs-target'); env['RUSTUP_HOME']=str(RUSTUP_HOME)
    return env

async def run_repo(repo, rows):
    root=(R/'repos'/os.environ.get('TOKEN_SAVE_RS_ROOT','hafley-rs-tokensave')) if repo=='hafley-rs' else R/'repos'/repo; LAB.mkdir(parents=True,exist_ok=True)
    env=env_for(root); (LAB/'tokensave-home').mkdir(parents=True,exist_ok=True)
    subprocess.run(['git','reset','--hard','HEAD'],cwd=root,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    if (root/'.tokensave').exists(): init_rc,init_out=0,''
    else: init_rc,init_out=call([str(BIN),'init','--no-git-hook',str(root)],R,env,1800)
    if init_rc:
        for row in rows: save(row,'tool_failed',f'TokenSave init exit {init_rc}: {init_out[-700:]}',0,0,0,int(row['timeout_seconds']))
        return
    params=StdioServerParameters(command=str(BIN),args=['serve','--path',str(root)],env=env,cwd=str(root))
    async with stdio_client(params) as (read,write):
        async with ClientSession(read,write) as session:
            await session.initialize()
            for row in rows:
                await one(session,root,row,env)

async def one(session,root,row,env):
    repo=row['repo']; key=f"{row['file']}#{row['name']}@{row['at_byte']}"; target_limit=int(row['timeout_seconds'])
    started=time.monotonic(); allowed=set(row['allowed_files'].split(';'))|{row['file']}; detail=''; status='tool_failed'; touched=set(); did_apply=False
    subprocess.run(['git','reset','--hard','HEAD'],cwd=root,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        line=(root/row['file']).read_bytes()[:int(row['at_byte'])].count(b'\n')+1
        lookup=await asyncio.wait_for(session.call_tool('tokensave_by_qualified_name',{'qualified_name':row['name']}),timeout=target_limit)
        items=json_text(lookup)
        if not isinstance(items,list): raise RuntimeError('by_qualified_name response did not contain a node list')
        candidates=[x for x in items if x.get('file','').replace('\\','/').endswith(row['file']) and x.get('line',x.get('start_line')) in (line,line-1)]
        if not candidates:
            raise RuntimeError(f'no TokenSave node matched {row["file"]}:{line} {row["name"]}')
        node=candidates[0]
        preview=await asyncio.wait_for(session.call_tool('tokensave_rename_preview',{'node_id':node['node_id']}),timeout=target_limit)
        pdata=json_text(preview)
        if not isinstance(pdata,dict) or not isinstance(pdata.get('references'),list): raise RuntimeError('rename_preview response had no references list')
        declared_line=int(node.get('line',line))
        lines={row['file']:{declared_line}}
        for ref in pdata['references']:
            if ref.get('direction')!='incoming' or not ref.get('file') or not ref.get('edge_line'): continue
            rel=pathlib.Path(ref['file']).as_posix()
            try: rel=pathlib.Path(ref['file']).resolve().relative_to(root).as_posix()
            except (ValueError,OSError):
                if rel.startswith(str(root).replace('\\','/')+'/'): rel=rel[len(str(root).replace('\\','/'))+1:]
            if not (root/rel).is_file(): continue
            lines.setdefault(rel,set()).add(int(ref['edge_line'])+1)
        new=row['name']+'_tokensave_renamed'
        for rel,nums in lines.items():
            p=root/rel; old=p.read_text(); arr=old.splitlines(keepends=True); changed=False
            for n in nums:
                if n<1 or n>len(arr): continue
                line_text=arr[n-1]
                replaced=re.sub(r'(?<![A-Za-z0-9_$])'+re.escape(row['name'])+r'(?![A-Za-z0-9_$])',new,line_text)
                if replaced!=line_text: arr[n-1]=replaced; changed=True
            updated=''.join(arr)
            if changed:
                result=await asyncio.wait_for(session.call_tool('tokensave_multi_str_replace',{'path':rel,'project_root':str(root),'replacements':[[old,updated]]}),timeout=target_limit)
                if result.isError: raise RuntimeError('multi_str_replace: '+str([getattr(x,'text','') for x in result.content])[:700])
                did_apply=True
        touched=touched_paths(root)
        if not did_apply: raise RuntimeError('preview did not yield editable declaration/reference lines')
        check_env=env.copy()
        if row['repo']=='hafley-rs' and row['file'].startswith('crates/sprefa-extract/'):
            check_env['CARGO_TARGET_DIR']=os.environ.get('TOKEN_SAVE_CARGO_TARGET_DIR',str(R/'repos/hafley-rs/build/sprefa-check'))
        check_rc,check_out=call(check_command(row),check_root(row,root),check_env,max(0.01,target_limit-(time.monotonic()-started)))
        if check_rc: status='typecheck_failed'; detail='TYPECHECK: '+check_out[-1100:]
        elif not touched.issubset(allowed): status='unexpected_files'; detail='touched paths outside graph file set: '+str(sorted(touched-allowed))
        else: status='pass'
    except asyncio.TimeoutError:
        status='timeout'; detail=f'per-target timeout after {target_limit}s'
    except subprocess.TimeoutExpired as exc:
        status='timeout'; detail=f'checker timeout after {target_limit}s: {str(exc)}'
    except Exception as exc:
        detail=f'{type(exc).__name__}: {exc}'
        if any(s in detail.lower() for s in ('must depend on','abstain','cannot ','refus','collision')): status='correct_refusal'
    touched|=touched_paths(root)
    restore(root,touched)
    save(row,status,detail,len(touched),len(allowed),time.monotonic()-started,target_limit)

def save(row,status,detail,touched,allowed,seconds,target_limit):
    key=f"{row['file']}#{row['name']}@{row['at_byte']}"
    DB.execute('INSERT OR REPLACE INTO corpus_score (repo,topology,operation,tool,mode,target,pass,timed_out,status,files_touched,allowed_files,seconds,detail,target_timeout_seconds) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
       (row['repo'],row['topology'],row['operation'],'tokensave', 'fast',key,int(status=='pass'),int(status=='timeout'),status,touched,allowed,round(seconds,3),detail[:2000],target_limit))
    DB.commit(); print(json.dumps({'repo':row['repo'],'target':key,'tool':'tokensave','status':status,'seconds':round(seconds,2)}),flush=True)

async def main():
    for repo in os.environ.get('TOKEN_SAVE_REPOS','hafley-rxjs,hafley-tsp,hafley-rs').split(','):
        rows=[x for x in ROWS if x['repo']==repo and x['operation']=='rename']
        await run_repo(repo,rows)
    DB.close()

asyncio.run(main())
