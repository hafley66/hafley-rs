import asyncio, json, os, sys, time
from pathlib import Path
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
ROOT=Path.home()/'.cache/lanes/claude-375/eval/serena'
async def main():
    project=Path(sys.argv[1]).resolve(); mode=sys.argv[2]
    lang=sys.argv[3]; rel=sys.argv[4]; name=sys.argv[5]
    env=dict(os.environ,SERENA_HOME=str(ROOT/'home'),UV_CACHE_DIR=str(ROOT/'cache/uv'),
             COURSIER_CACHE=str(ROOT/'cache/coursier'),GOCACHE=str(ROOT/'cache/go-build'),
             GOPATH=str(ROOT/'cache/gopath'),GOBIN=str(ROOT/'bin'),PATH=f"{ROOT/'bin'}:"+os.environ['PATH'],DO_NOT_TRACK='1')
    argv=['start-mcp-server','--project',str(project),'--context',os.getenv('SERENA_CONTEXT','claude-code'),'--language-backend','LSP',
          '--enable-web-dashboard','False','--open-web-dashboard','False']
    server=StdioServerParameters(command=str(ROOT/'.venv/bin/serena'),args=argv,env=env,cwd=str(project))
    t0=time.monotonic(); rows=[]
    try:
      async with stdio_client(server) as (read,write):
       async with ClientSession(read,write) as session:
        await session.initialize(); startup=time.monotonic()-t0
        available=await session.list_tools()
        if mode=='schema':
         out=ROOT/'mcp-tools.json'; out.write_text(json.dumps([{'name':x.name,'description':x.description,'inputSchema':x.inputSchema} for x in available.tools],indent=2))
         print(json.dumps({'startup_s':startup,'tools':len(available.tools),'schema_file':str(out)})); return
        tool={'refs':'find_referencing_symbols','rename':'rename_symbol','find':'find_symbol'}[mode]
        args={'name_path':name,'relative_path':rel}
        if mode=='find': args={'name_path_pattern':name,'relative_path':rel,'include_info':True,'max_matches':20}
        if mode=='rename':
         args['new_name']=sys.argv[6]
         if len(sys.argv)>7: args['name_path']=sys.argv[7]
        t1=time.monotonic()
        result=await asyncio.wait_for(session.call_tool(tool,args),timeout=300)
        row={'startup_s':startup,'wall_s':time.monotonic()-t1,'tool':tool,'args':args,'isError':result.isError,
             'content':[{'type':c.type,'text':getattr(c,'text',None)} for c in result.content]}
        print(json.dumps(row,ensure_ascii=False))
    except Exception as e:
      print(json.dumps({'startup_s':time.monotonic()-t0,'tool':mode,'args':{'relative_path':rel,'name_path':name},'error':type(e).__name__+': '+str(e)}))
asyncio.run(main())
