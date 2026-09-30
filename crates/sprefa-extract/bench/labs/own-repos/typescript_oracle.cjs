const fs = require('node:fs');
const path = require('node:path');
const ts = require(path.resolve('tooling/tsserver/node_modules/typescript'));
if (ts.version !== '5.9.3') throw new Error(`expected TypeScript 5.9.3, got ${ts.version}`);
const rows = fs.readFileSync('targets.tsv','utf8').trimEnd().split('\n').slice(1).map(line => {
  const c=line.split('\t'); return {repo:c[0],operation:c[2],file:c[3],name:c[4],atByte:Number(c[5]),dest:c[6],check_package:c[14]};
});
function nearestConfig(target, root) {
  let dir=path.dirname(target);
  while (dir.startsWith(root)) {
    for (const name of ['tsconfig.json','jsconfig.json','tsconfig.build.json']) if (fs.existsSync(path.join(dir,name))) return path.join(dir,name);
    const parent=path.dirname(dir); if (parent===dir) break; dir=parent;
  }
  return path.join(root,'tsconfig.json');
}
function serviceFor(configPath) {
  const config=ts.readConfigFile(configPath,ts.sys.readFile);
  if (config.error) throw new Error(ts.flattenDiagnosticMessageText(config.error.messageText,'\n'));
  const parsed=ts.parseJsonConfigFileContent(config.config,ts.sys,path.dirname(configPath),{},configPath);
  const versions=new Map();
  const host={
    getCompilationSettings:()=>parsed.options,
    getScriptFileNames:()=>parsed.fileNames,
    getScriptVersion:file=>versions.get(file)||'0',
    getScriptSnapshot:file=>fs.existsSync(file)?ts.ScriptSnapshot.fromString(fs.readFileSync(file,'utf8')):undefined,
    getCurrentDirectory:()=>path.dirname(configPath),
    getDefaultLibFileName:options=>ts.getDefaultLibFilePath(options),
    fileExists:ts.sys.fileExists,readFile:ts.sys.readFile,readDirectory:ts.sys.readDirectory,
    directoryExists:ts.sys.directoryExists, getDirectories:ts.sys.getDirectories,
  };
  return ts.createLanguageService(host,ts.createDocumentRegistry());
}
function fixture(row) {
 const root=path.resolve('repos',row.repo), file=path.resolve(root,row.file), config=nearestConfig(file,root), ls=serviceFor(config);
 const content=fs.readFileSync(file); const byte=row.atByte;
 const position=Buffer.from(content.subarray(0,byte)).toString('utf8').length;
 const info=ls.getRenameInfo(file,position,{allowRenameOfImportPath:true});
 if (row.operation==='rename') {
  const locations=info.canRename?ls.findRenameLocations(file,position,false,false,true)||[]:[];
  return {repo:row.repo,operation:row.operation,file:row.file,name:row.name,atByte:row.atByte,typescript:ts.version,tsconfig:path.relative(root,config),renameInfo:info,locations:locations.map(x=>({fileName:x.fileName,textSpan:x.textSpan,contextSpan:x.contextSpan}))};
 }
 const refs=ls.getApplicableRefactors(file,{pos:position,end:position+row.name.length},{allowTextChangesInNewFiles:true},undefined,undefined,true);
 const found=[];
 for (const refactor of refs) for (const action of refactor.actions||[]) {
  if (/move to file/i.test(refactor.name+' '+action.name+' '+action.description)) {
   try { const result=ls.getEditsForRefactor(file,{}, {pos:position,end:position+row.name.length},refactor.name,action.name,{allowTextChangesInNewFiles:true},{targetFile:path.resolve(root,row.dest)}); found.push({name:refactor.name,action:action.name,description:action.description,result:result&&{edits:result.edits?.map(e=>({file:e.fileName,textChanges:e.textChanges})),renameFilename:result.renameFilename,notApplicableReason:result.notApplicableReason}}); }
   catch(e) { found.push({name:refactor.name,action:action.name,error:String(e)}); }
  }
 }
 const edits=found.flatMap(x=>x.result?.edits||[]).map(e=>({file:path.relative(root,e.file),textChanges:e.textChanges}));
 return {repo:row.repo,operation:row.operation,file:row.file,name:row.name,atByte:row.atByte,typescript:ts.version,tsconfig:path.relative(root,config),moveOptions:found.map(x=>({name:x.name,action:x.action,error:x.error,notApplicableReason:x.result?.notApplicableReason})),edits};
}
for (const row of rows.filter(x=>['hafley-rxjs','hafley-tsp'].includes(x.repo))) {
 try {
  const data=fixture(row); const stem=`${row.repo}__${row.operation}__${row.file.replaceAll('/','_')}__${row.name}__${row.atByte}.json`;
  const lane=path.join('oracles','ts'); fs.mkdirSync(lane,{recursive:true}); fs.writeFileSync(path.join(lane,stem),JSON.stringify(data,null,2)+'\n');
  console.log(JSON.stringify({repo:row.repo,operation:row.operation,target:`${row.file}#${row.name}@${row.atByte}`,canRename:data.renameInfo?.canRename,moveOptions:data.edits?.length,tsconfig:data.tsconfig}));
 } catch(e) { console.log(JSON.stringify({repo:row.repo,operation:row.operation,target:`${row.file}#${row.name}@${row.atByte}`,error:String(e)})); }
}
