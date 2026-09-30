const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process')
const root=process.cwd(),ts=require(path.join(root,'tools/tsserver593/node_modules/typescript'))
const rows=fs.readFileSync(path.join(root,'targets.tsv'),'utf8').trim().split('\n').slice(1).map(x=>x.split('\t'))
const cache=new Map()
function serviceFor(source){
 const configPath=ts.findConfigFile(path.dirname(source),ts.sys.fileExists)
 if(!configPath) return {error:'no tsconfig'}
 if(cache.has(configPath))return cache.get(configPath)
 const config=ts.getParsedCommandLineOfConfigFile(configPath,{}, {...ts.sys,onUnRecoverableConfigFileDiagnostic(d){throw Error(ts.flattenDiagnosticMessageText(d.messageText,'\n'))}})
 const host={getCompilationSettings:()=>config.options,getScriptFileNames:()=>config.fileNames,getScriptVersion:()=> '0',getScriptSnapshot(f){try{return ts.ScriptSnapshot.fromString(fs.readFileSync(f,'utf8'))}catch{return undefined}},getCurrentDirectory:()=>path.dirname(configPath),getDefaultLibFileName:o=>ts.getDefaultLibFilePath(o),fileExists:ts.sys.fileExists,readFile:ts.sys.readFile,readDirectory:ts.sys.readDirectory,directoryExists:ts.sys.directoryExists,getDirectories:ts.sys.getDirectories,realpath:ts.sys.realpath}
 const v=ts.createLanguageService(host,ts.createDocumentRegistry());cache.set(configPath,v);return v
}
for(const [repo,topology,operation,file,name,atByte,dest,destNew,crossPackage] of rows){
 if(!['ajv','codegraph-src','vite'].includes(repo))continue
 const repoRoot=path.join(root,'repos',repo),source=path.join(repoRoot,file),sourceText=fs.readFileSync(source,'utf8'),bytePosition=Number(atByte),position=Buffer.from(sourceText).subarray(0,bytePosition).toString('utf8').length,ls=serviceFor(source),configPath=ts.findConfigFile(path.dirname(source),ts.sys.fileExists),out={repo,operation,file,name,atByte:bytePosition,typescript:ts.version,tsconfig:configPath?path.relative(repoRoot,configPath):null,dest}
 try{
  if(ls.error)throw Error(ls.error)
  if(operation==='rename'){
   const info=ls.getRenameInfo(source,position,{allowRenameOfImportPath:false})
   out.renameInfo=info
   out.locations=info.canRename?ls.findRenameLocations(source,position,false,false,true):[]
  }else{
   const sf=ls.getProgram()?.getSourceFile(source)
   if(!sf)throw Error(`source file is absent from TypeScript project ${out.tsconfig}`)
   out.projectLoaded=true
   out.destinationInProject=dest?!!ls.getProgram()?.getSourceFile(path.join(repoRoot,dest)):false
   const token=ts.getTokenAtPosition(sf,position)
   const targetKinds=new Set(['FunctionDeclaration','ClassDeclaration','InterfaceDeclaration','TypeAliasDeclaration','EnumDeclaration','VariableDeclaration'].map(k=>ts.SyntaxKind[k]))
   let decl=token;while(decl&&decl!==sf&&!targetKinds.has(decl.kind))decl=decl.parent
   if(!decl||decl===sf)throw Error(`declaration not found at byte offset ${bytePosition} (TypeScript offset ${position})`)
   const stmt=ts.isVariableDeclaration(decl)?decl.parent.parent:decl
   if(!ts.isStatement(stmt)||stmt.parent!==sf)throw Error(`target declaration is not top-level: ${ts.SyntaxKind[stmt.kind]}`)
   out.targetKind=ts.SyntaxKind[decl.kind];out.statementKind=ts.SyntaxKind[stmt.kind]
   const pos=decl.name?.getStart(sf)??decl.getStart(sf)
   const app=ls.getApplicableRefactors(source,pos,{allowTextChangesInNewFiles:true},undefined,undefined,true)||[]
   const action=app.flatMap(r=>(r.actions||[]).map(a=>({r,a}))).find(x=>String(destNew)==='true'?x.r.name==='Move to a new file':x.r.name==='Move to file')
   out.applicable=app.map(r=>({name:r.name,description:r.description,actions:r.actions}))
   if(action){const result=ls.getEditsForRefactor(source,{indentSize:2,tabSize:2,newLineCharacter:'\n',convertTabsToSpaces:true},{pos:stmt.getStart(sf),end:stmt.end},action.r.name,action.a.name,{allowTextChangesInNewFiles:true},String(destNew)==='true'?undefined:{targetFile:path.join(repoRoot,dest)});out.selected={refactor:action.r.name,action:action.a.name};out.edits=result?.edits?.map(e=>({file:path.relative(repoRoot,e.fileName),textChanges:e.textChanges}))||[]}
  }
 }catch(e){out.error=String(e&&e.message||e)}
 const id=`${repo}__${operation}__${file.replaceAll('/','_')}__${name}__${bytePosition}.json`
 fs.writeFileSync(path.join(root,'oracles/ts',id),JSON.stringify(out,null,2)+'\n')
}
