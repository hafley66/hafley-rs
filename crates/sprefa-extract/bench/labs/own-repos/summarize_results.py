import csv, sqlite3
from collections import Counter, defaultdict
from pathlib import Path
R=Path.cwd()
db=sqlite3.connect(R/'results.db')
db.row_factory=sqlite3.Row
rows=db.execute('select * from corpus_score order by repo, operation, tool, mode, target').fetchall()
fail_status={'tool_failed','typecheck_failed','unexpected_files'}
counts=defaultdict(Counter)
for r in rows: counts[(r['repo'],r['operation'],r['tool'],r['mode'])][r['status']]+=1
with open(R/'harness/scratch/results-summary.tsv','w') as f:
 w=csv.writer(f,delimiter='\t',lineterminator='\n')
 w.writerow(['repository','operation','tool','mode','targets_scored','pass','correct_refusal','fail','timeout','oracle_unavailable'])
 for key,c in sorted(counts.items()):
  w.writerow([*key,sum(c.values()),c['pass'],c['correct_refusal'],sum(c[s] for s in fail_status),c['timeout'],c['oracle_unavailable']])

def cause(r):
 s=r['status']; d=(r['detail'] or '').lower()
 if s=='timeout': return 'target exceeded baseline-derived timeout'
 if s=='typecheck_failed': return 'post-edit typecheck failed'
 if s=='unexpected_files': return 'tool edited files outside the allowed set'
 if 'dependency cycle' in d: return 'move would create a dependency cycle'
 if 'declares no' in d: return 'tool could not resolve a declaration at the selected target'
 if 'no edits' in d or 'no in-repository edits' in d: return 'tool returned no applicable edits'
 if 'must depend on' in d: return 'tool refused a package dependency direction'
 return 'tool operation failed'
def extract(r):
 d=r['detail'] or ''
 diff=[]; in_diff=False
 for line in d.splitlines():
  stripped=line.lstrip()
  if stripped.startswith('--- ') or stripped.startswith('+++ '):
   in_diff=True
   continue
  if (in_diff or stripped.startswith(('@@','-','+'))) and stripped.startswith(('@@','-','+')):
   if not stripped.startswith(('---','+++')):
    diff.append(stripped[:240])
   if len(diff)>=2: break
 diag='not run'
 if 'TYPECHECK_ERRORS:' in d:
  marker=d.split('TYPECHECK_ERRORS:',1)[1].strip()
  diag=marker.split(' | ',1)[0][:280] or 'typecheck failed without a retained diagnostic'
 elif 'TYPECHECK:' in d:
  td=d.split('TYPECHECK:',1)[1]
  for line in td.splitlines():
   if 'error[' in line.lower() or 'error TS' in line or line.lstrip().startswith('error:'):
    diag=line.strip()[:280]; break
  if diag=='not run': diag='no error line retained'
 elif r['status']=='unexpected_files': diag='typecheck passed; changed-file set failed'
 return ' | '.join(diff) if diff else 'none (tool made no source diff)',diag
items=defaultdict(list)
for r in rows:
 if r['status'] in fail_status or r['status']=='timeout': items[(r['repo'],r['operation'],r['tool'],r['mode'],cause(r))].append(r)
with open(R/'harness/scratch/failure-examples.tsv','w') as f:
 w=csv.writer(f,delimiter='\t',lineterminator='\n')
 w.writerow(['repository','operation','tool','mode','failure_cause','count','example_target','diff_example','typecheck_line'])
 for key,examples in sorted(items.items()):
  r=examples[0]; diff,diag=extract(r)
  w.writerow([*key,len(examples),r['target'],diff,diag])
