/** TypeScript type references to the final declaration behind import aliases. */
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

TypeDecl canonicalDeclaration(LocalTypeName name) {
  result = name.getADeclaration() and
  not exists(ImportSpecifier spec | result = spec.getLocal())
  or
  exists(ImportSpecifier spec, LocalTypeName exported |
    name.getADeclaration() = spec.getLocal() and
    spec.getImportDeclaration().getImportedModule().(ES2015Module)
      .exportsAs(exported, spec.getImportedName()) and
    result = canonicalDeclaration(exported)
  )
}

from LocalTypeAccess site, TypeDecl target, AstNode source
where
  target = canonicalDeclaration(site.getLocalTypeName()) and
  source = owner(site) and
  exists(site.getFile().getRelativePath()) and
  exists(target.getFile().getRelativePath())
select site.getFile().getRelativePath() as src_file,
  itemName(source) as enclosing_item,
  target.getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
