/** TypeScript type references to declarations in the corpus. */
import javascript
import semmle.javascript.ES2015Modules
import semmle.javascript.TypeScript

string itemName(AstNode item) {
  result = item.(Function).getName()
  or result = item.(ClassDefinition).getName()
  or result = item.(InterfaceDeclaration).getName()
  or result = item.(TypeAliasDeclaration).getName()
}

AstNode owner(LocalTypeAccess site) {
  result = site.getParent+() and exists(itemName(result)) and
  not exists(AstNode nearer |
    nearer = site.getParent+() and exists(itemName(nearer)) and
    nearer.getParent+() = result
  )
}

TypeDecl destination(LocalTypeAccess site) {
  result = site.getLocalTypeName().getADeclaration() and
  not exists(ImportSpecifier spec | result = spec.getLocal())
  or
  exists(ImportSpecifier spec, LocalTypeName name |
    site.getLocalTypeName().getADeclaration() = spec.getLocal() and
    spec.getImportDeclaration().getImportedModule().(ES2015Module)
      .exportsAs(name, spec.getImportedName()) and
    result = name.getADeclaration()
  )
}

from LocalTypeAccess site, TypeDecl target, AstNode source
where
  target = destination(site) and
  source = owner(site) and
  exists(site.getFile().getRelativePath()) and
  exists(target.getFile().getRelativePath())
select site.getFile().getRelativePath() as src_file,
  itemName(source) as enclosing_item,
  target.getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
