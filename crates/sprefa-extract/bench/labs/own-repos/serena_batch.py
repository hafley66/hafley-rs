import asyncio,csv,json,os,pathlib,sqlite3,subprocess,time
from mcp import ClientSession,StdioServerParameters
from mcp.client.stdio import stdio_client
R=pathlib.Path.cwd(); rows=list(csv.DictReader(open(os.environ.get('SERENA_TARGETS_FILE',R/'targets.tsv')),delimiter='\t'))
DB=sqlite3.connect(R/'results.db')
DB.execute('''CREATE TABLE IF NOT EXISTS corpus_score (repo TEXT,topology TEXT,operation TEXT,tool TEXT,mode TEXT,target TEXT,pass INTEGER,timed_out INTEGER,status TEXT,files_touched INTEGER,allowed_files INTEGER,seconds REAL,detail TEXT,target_timeout_seconds INTEGER,PRIMARY KEY(repo,operation,tool,mode,target))'''); DB.commit()
LAB=R/'harness/scratch/competitors/serena'; EVAL=pathlib.Path.home()/'.cache/lanes/claude-375/eval/serena'
def command(cmd,cwd,env,timeout): return subprocess.run(cmd,cwd=cwd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=timeout)
def changed(root):
 p=command(['git','status','--porcelain','--untracked-files=all'],root,os.environ.copy(),30)
 return {line[3:].split(' -> ')[-1] for line in p.stdout.splitlines() if line[3:] and not any(x in line[3:].split('/') for x in ('.git','node_modules','.tokensave','.serena','.cargo-home','build'))}
def restore(root,touched):
 command(['git','reset','--hard','HEAD'],root,os.environ.copy(),30)
 for rel in touched:
  path=root/rel
  if command(['git','ls-files','--error-unmatch',rel],root,os.environ.copy(),30).returncode==0: continue
  if path.is_dir():
   import shutil; shutil.rmtree(path)
  elif path.exists(): path.unlink()
def checker(row):
 if row['repo']=='hafley-rxjs': return ['pnpm','--filter',row['check_package'],'typecheck']
 if row['repo']=='hafley-tsp': return ['pnpm','--filter',row['check_package'],'build']
 if row['file'].startswith('crates/sprefa-extract/'): return ['cargo','check','--features','cli','--locked','-j','2']
 return ['cargo','check','--workspace','--all-targets','--locked','-j','2']
def checker_root(row,root):
 if row['repo']=='hafley-rs' and row['file'].startswith('crates/sprefa-extract/'): return root/'crates'/'sprefa-extract'
 return root
def save(row,status,detail,touched,allowed,seconds,limit):
 key=f"{row['file']}#{row['name']}@{row['at_byte']}"
 DB.execute('INSERT OR REPLACE INTO corpus_score VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)',(row['repo'],row['topology'],'rename','serena','rename',key,int(status=='pass'),int(status=='timeout'),status,len(touched),len(allowed),round(seconds,3),detail[:1800],limit)); DB.commit()
 print(json.dumps({'repo':row['repo'],'target':key,'tool':'serena','status':status,'seconds':round(seconds,2)}),flush=True)
async def run_repo(repo):
 root=(R/'repos'/os.environ.get('SERENA_RS_ROOT','hafley-rs-competitor')) if repo=='hafley-rs' else R/'repos'/repo; LAB.mkdir(parents=True,exist_ok=True)
 env=dict(os.environ,SERENA_HOME=str(LAB/'home'),UV_CACHE_DIR=str(LAB/'cache/uv'),COURSIER_CACHE=str(LAB/'cache/coursier'),GOCACHE=str(LAB/'cache/go-build'),GOPATH=str(LAB/'cache/gopath'),GOBIN=str(LAB/'bin'),DO_NOT_TRACK='1',npm_config_cache=str(LAB/'cache/npm'),NPM_CONFIG_CACHE=str(LAB/'cache/npm'))
 if repo=='hafley-rs': env.update(CARGO_HOME=str(R/'repos/hafley-rs/.cargo-home'),CARGO_TARGET_DIR=str(root/'build/rs-target'))
 env['PATH']=f"{EVAL/'bin'}:{LAB/'bin'}:"+env.get('PATH','')
 subprocess.run(['git','reset','--hard','HEAD'],cwd=root,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 language='rust' if repo=='hafley-rs' else 'typescript'
 server=StdioServerParameters(command=str(EVAL/'.venv/bin/serena'),args=['start-mcp-server','--project',str(root),'--context','claude-code','--language-backend','LSP','--enable-web-dashboard','False','--open-web-dashboard','False'],env=env,cwd=str(root))
 async with stdio_client(server) as (read,write):
  async with ClientSession(read,write) as session:
   await session.initialize()
   for row in [x for x in rows if x['repo']==repo and x['operation']=='rename']:
    limit=int(row['timeout_seconds']); start=time.monotonic(); deadline=start+limit; allowed=set(row['allowed_files'].split(';'))|{row['file']}; touched=set(); detail=''; status='tool_failed'
    command(['git','reset','--hard','HEAD'],root,env,30)
    try:
     result=await asyncio.wait_for(session.call_tool('rename_symbol',{'name_path':row['name'],'relative_path':row['file'],'new_name':row['name']+'_serena_renamed'}),timeout=limit)
     detail=json.dumps([getattr(c,'text','') for c in result.content])[:1200]
     touched=changed(root)
     message=' '.join(getattr(c,'text','') for c in result.content).lower()
     refusal=any(w in message for w in ('must depend on','abstain','cannot ','refus','collision','not supported','found multiple','ambiguous'))
     if result.isError and refusal: status='correct_refusal'
     elif result.isError or not touched: status='tool_failed'
     else:
      check=command(checker(row),checker_root(row,root),env,max(.01,deadline-time.monotonic()))
      if check.returncode: status='typecheck_failed'; detail+=' TYPECHECK: '+check.stdout[-850:]
      elif not touched.issubset(allowed): status='unexpected_files'; detail+=' outside allowed files: '+str(sorted(touched-allowed))
      else: status='pass'
    except asyncio.TimeoutError: status='timeout'; detail=f'per-target timeout after {limit}s'
    except subprocess.TimeoutExpired as e: status='timeout'; detail=f'checker timeout after {limit}s: {e}'
    except Exception as e:
     detail=f'{type(e).__name__}: {e}'
     if any(w in detail.lower() for w in ('must depend on','abstain','cannot ','refus','collision','not supported','found multiple','ambiguous')): status='correct_refusal'
    touched|=changed(root); restore(root,touched); save(row,status,detail,touched,allowed,time.monotonic()-start,limit)
for repo in os.environ.get('SERENA_REPOS','hafley-rs,hafley-rxjs,hafley-tsp').split(','): asyncio.run(run_repo(repo))
DB.close()
