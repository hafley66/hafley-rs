import asyncio, json, os, sys, time
from pathlib import Path
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

EVAL = Path.home()/'.cache/lanes/claude-375/eval/serena'
LAB = Path.cwd()/'harness'/'scratch'/'competitors'/'serena'

async def main():
    project=Path(sys.argv[1]).resolve(); lang=sys.argv[2]; rel=sys.argv[3]; name=sys.argv[4]; new=sys.argv[5]
    env=dict(os.environ, SERENA_HOME=str(LAB/'home'), UV_CACHE_DIR=str(LAB/'cache'/'uv'),
             COURSIER_CACHE=str(LAB/'cache'/'coursier'), GOCACHE=str(LAB/'cache'/'go-build'),
             GOPATH=str(LAB/'cache'/'gopath'), GOBIN=str(LAB/'bin'), DO_NOT_TRACK='1',
             npm_config_cache=str(LAB/'cache'/'npm'), NPM_CONFIG_CACHE=str(LAB/'cache'/'npm'))
    env['PATH']=f"{EVAL/'bin'}:{LAB/'bin'}:"+env.get('PATH','')
    args=['start-mcp-server','--project',str(project),'--context','claude-code',
          '--language-backend','LSP','--enable-web-dashboard','False','--open-web-dashboard','False']
    server=StdioServerParameters(command=str(EVAL/'.venv/bin/serena'),args=args,env=env,cwd=str(project))
    started=time.monotonic()
    try:
        async with stdio_client(server) as (read,write):
            async with ClientSession(read,write) as session:
                await session.initialize()
                begin=time.monotonic()
                result=await asyncio.wait_for(session.call_tool('rename_symbol',{'name_path':name,'relative_path':rel,'new_name':new}),timeout=600)
                print(json.dumps({'startup_s':round(begin-started,3),'wall_s':round(time.monotonic()-begin,3),
                                  'isError':result.isError,'content':[getattr(x,'text','') for x in result.content]}))
    except Exception as exc:
        print(json.dumps({'error':type(exc).__name__+': '+str(exc),'wall_s':round(time.monotonic()-started,3)}))

asyncio.run(main())
