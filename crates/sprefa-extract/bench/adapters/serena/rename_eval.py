import json, os, shutil, sqlite3, subprocess, time
from pathlib import Path
E=Path.home()/'.cache/lanes/claude-375/eval'
DB=sqlite3.connect(E/'bench/truth.db')
RUNS=E/'bench/runs/serena'; COPIES=E/'serena/rename-repos'; OUT=E/'serena/rename-results.jsonl'
REPOS=('codegraph-src','hafley-rs','hafley_scm','tokio','vite')
LANG={'codegraph-src':'ts','vite':'ts','hafley-rs':'rust','hafley_scm':'rust','tokio':'rust'}
PREFIX={'hafley_scm':'crates/hafley_scm/'}
ENV=dict(os.environ, CARGO_TARGET_DIR=str(Path.home()/'.cache/lanes/claude-375/target'), CARGO_BUILD_JOBS='2', DO_NOT_TRACK='1')

def run(args,cwd,timeout=1800):
 return subprocess.run(args,cwd=cwd,env=ENV,text=True,capture_output=True,timeout=timeout)

def diff_lines(root):
 p=run(['git','diff','--no-ext-diff','--unified=0'],root)
 deleted={}; added={}; current=None; oldline=newline=None
 for line in p.stdout.splitlines():
  if line.startswith('diff --git a/'):
   current=line.split(' b/',1)[1]; deleted.setdefault(current,[]); added.setdefault(current,[])
  elif line.startswith('@@'):
   import re
   m=re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@',line)
   if m: oldline=int(m.group(1)); newline=int(m.group(3))
  elif current and line.startswith('--- '): pass
  elif current and line.startswith('+++ '): pass
  elif current and line.startswith('-'):
   deleted[current].append((oldline,line[1:])); oldline+=1
  elif current and line.startswith('+'):
   added[current].append((newline,line[1:])); newline+=1
  elif current and line.startswith(' '):
   oldline+=1; newline+=1
 return deleted,added,p.returncode,p.stderr

def check_cmd(repo,root):
 if LANG[repo]=='rust': return ['cargo','check','-j','2']
 return ['pnpm','exec','tsc','--noEmit']

done=set()
if OUT.exists():
 existing=[json.loads(line) for line in OUT.read_text().splitlines() if line.strip()]
 for row in existing:
  row['diff_only_sites']=bool(row['truth_sites']) and row['exact_sites'] and len(row['diff_files'])==row['expected_files']
  done.add((row['repo'],row['symbol']))
 OUT.write_text(''.join(json.dumps(row,ensure_ascii=False)+'\n' for row in existing))
with OUT.open('a') as out:
 for repo in REPOS:
  src=RUNS/'hafley-rs' if repo=='hafley_scm' else RUNS/repo
  dst=COPIES/repo
  if not (dst/'.git').exists():
   dst.parent.mkdir(parents=True,exist_ok=True)
   p=run(['git','clone','--shared',str(src),str(dst)],E)
   if p.returncode: raise RuntimeError(p.stderr)
  targets=DB.execute('select symbol,name,kind,def_path from target where repo=? order by kind,def_path,name',(repo,)).fetchall()
  for i,(symbol,name,kind,path) in enumerate(targets,1):
   if (repo,symbol) in done: continue
   # Restore every tracked source file to the clean clone state before each dry-run rename.
   run(['git','reset','--hard','HEAD'],dst)
   cache=dst/'.serena'
   if cache.exists(): shutil.rmtree(cache)
   rel=PREFIX.get(repo,'')+path
   new=name+'_zz'
   call=run([str(E/'serena/.venv/bin/python'),str(E/'serena/mcp_call.py'),str(dst),'rename',LANG[repo],rel,name,new],dst,timeout=600)
   payload=None
   for line in reversed(call.stdout.splitlines()):
    if line.startswith('{'):
     try:
      candidate=json.loads(line)
      if candidate.get('tool')=='rename_symbol' or 'error' in candidate: payload=candidate; break
     except Exception: pass
   occ=DB.execute('select path,line from occ where repo=? and symbol=?',(repo,symbol)).fetchall()
   expected={(PREFIX.get(repo,'')+p,int(line)) for p,line in occ}
   expected_files={p for p,_ in expected}
   deleted,added,dr,derr=diff_lines(dst)
   actual={(p,line) for p,items in deleted.items() for line,_ in items}
   touched={p for p,items in deleted.items() if items}|{p for p,items in added.items() if items}
   exact_sites=bool(expected) and actual==expected
   diff_only=bool(expected) and actual==expected and touched==expected_files
   check_started=time.monotonic()
   chk=run(check_cmd(repo,dst),dst,timeout=1800)
   check_wall=time.monotonic()-check_started
   row={'repo':repo,'symbol':symbol,'name':name,'kind':kind,'path':path,'new_name':new,
        'mcp_client_rc':call.returncode,'serena_error':payload.get('error') if payload else (call.stderr[-1000:] or 'no JSON tool result'),
        'serena_result':payload.get('content') if payload else None,'serena_wall_s':payload.get('wall_s') if payload else None,
        'truth_sites':len(expected),'edited_lines':len(actual),'diff_files':sorted(touched),
        'expected_files':len(expected_files),'exact_sites':exact_sites,'diff_only_sites':diff_only,
        'check_command':check_cmd(repo,dst),'check_rc':chk.returncode,'check_wall_s':round(check_wall,3),
        'check_output':(chk.stdout+chk.stderr)[-1500:]}
   out.write(json.dumps(row,ensure_ascii=False)+'\n'); out.flush(); done.add((repo,symbol))
   print(f'{repo:14} {kind:6} {name[:30]:30} mcp={call.returncode} sites={int(exact_sites)} diff={int(diff_only)} check={chk.returncode} {check_wall:.2f}s',flush=True)
