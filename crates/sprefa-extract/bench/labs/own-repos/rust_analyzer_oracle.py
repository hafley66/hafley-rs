import csv, json, os, pathlib, queue, subprocess, sys, threading, time

R = pathlib.Path.cwd()
rows = [x for x in csv.DictReader(open(R/'targets.tsv'), delimiter='\t') if x['repo']=='hafley-rs' and x['operation']=='rename']
if os.environ.get('RA_SMOKE'):
    at=int(os.environ['RA_SMOKE']); rows=rows[at-1:at]
root = R/'repos/hafley-rs-competitor'
server = pathlib.Path.home()/'.rustup/toolchains/nightly-aarch64-apple-darwin/bin/rust-analyzer'
server_log = open(R/'harness/scratch/rust-analyzer-server.stderr.log','wb')
server_env=os.environ.copy(); server_env['CARGO_HOME']=str(root/'.cargo-home'); server_env['CARGO_TARGET_DIR']=str(R/'build'/'rust-analyzer-target')
proc = subprocess.Popen([str(server)], cwd=root, env=server_env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=server_log)
messages = queue.Queue()

def read_loop():
    while True:
        headers = {}
        while True:
            line = proc.stdout.readline()
            if not line: return
            if line in (b'\r\n', b'\n'): break
            key, value = line.decode('ascii').split(':', 1)
            headers[key.lower()] = value.strip()
        length = int(headers['content-length'])
        body = proc.stdout.read(length)
        try: messages.put(json.loads(body))
        except Exception as error: messages.put({'_error':str(error),'_body':body.decode(errors='replace')})

threading.Thread(target=read_loop,daemon=True).start()
next_id = 1

def send(message):
    body=json.dumps(message,separators=(',',':')).encode()
    proc.stdin.write(f'Content-Length: {len(body)}\r\n\r\n'.encode()+body); proc.stdin.flush()

def respond_to_server(message):
    method=message.get('method')
    params=message.get('params') or {}
    if method=='workspace/configuration': result=[None for _ in params.get('items',[])]
    elif method=='workspace/workspaceFolders': result=[{'uri':root.as_uri(),'name':'hafley-rs'}]
    else: result=None
    send({'jsonrpc':'2.0','id':message['id'],'result':result})

def request(method, params, timeout=957):
    global next_id
    ident=next_id; next_id+=1; send({'jsonrpc':'2.0','id':ident,'method':method,'params':params})
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        try: message=messages.get(timeout=min(2,end-time.monotonic()))
        except queue.Empty: continue
        if 'method' in message and 'id' in message: respond_to_server(message); continue
        if message.get('id')==ident: return message
    raise TimeoutError(method)

init=request('initialize',{
 'processId':os.getpid(),'rootUri':root.as_uri(),'workspaceFolders':[{'uri':root.as_uri(),'name':'hafley-rs'}],
 'capabilities':{'workspace':{'workspaceEdit':{'documentChanges':True}},'textDocument':{'rename':{'dynamicRegistration':False}}},
 'initializationOptions':{'checkOnSave':{'command':'clippy','enable':False}},
})
print(json.dumps({'event':'initialize','error':init.get('error')}),flush=True)
send({'jsonrpc':'2.0','method':'initialized','params':{}})
time.sleep(100)

for index,row in enumerate(rows,1):
    file=root/row['file']; text=file.read_text(); byte=int(row['at_byte']); prefix=text.encode()[:byte].decode('utf-8')
    line=prefix.count('\n'); before=prefix.rsplit('\n',1)[-1]; character=len(before.encode('utf-16-le'))//2
    uri=file.as_uri(); send({'jsonrpc':'2.0','method':'textDocument/didOpen','params':{'textDocument':{'uri':uri,'languageId':'rust','version':1,'text':text}}})
    try:
        location={'textDocument':{'uri':uri},'position':{'line':line,'character':character}}
        prepare=request('textDocument/prepareRename',location)
        response=request('textDocument/rename',{**location,'newName':row['name']+'_oracle_renamed'})
        result=response.get('result')
        error=response.get('error')
    except Exception as exc:
        result=None; error={'message':str(exc)}; prepare={'result':None}
    data={'repo':'hafley-rs','operation':'rename','file':row['file'],'name':row['name'],'atByte':byte,'response':{'result':result},'prepareRename':prepare.get('result'),'error':error}
    stem=f"hafley-rs__rename__{row['file'].replace('/','_')}__{row['name']}__{byte}.json"
    out=R/'oracles/rust'/stem; out.parent.mkdir(parents=True,exist_ok=True); out.write_text(json.dumps(data,indent=2)+'\n')
    print(json.dumps({'i':index,'total':len(rows),'target':f"{row['file']}#{row['name']}@{byte}",'prepared':bool(prepare.get('result')),'edits':bool(result),'error':error}),flush=True)
    send({'jsonrpc':'2.0','method':'textDocument/didClose','params':{'textDocument':{'uri':uri}}})

try: request('shutdown',{},timeout=10)
except Exception: pass
send({'jsonrpc':'2.0','method':'exit','params':{}})
proc.terminate(); proc.wait(timeout=10)
