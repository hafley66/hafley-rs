/** Root type comparison with enclosing syntax; nested references can carry
 * several labels. The A3 report assigns the first matching label in this
 * order: field, parameter, return, local, generic, alias, impl, other. */
import rust
import codeql.rust.internal.PathResolution

string ownerName(Item owner) {
  result = owner.(Struct).getName().getText()
  or result = owner.(Enum).getName().getText()
  or result = owner.(Function).getName().getText()
  or result = owner.(TypeAlias).getName().getText()
  or result = owner.(Impl).getSelfTy().(PathTypeRepr).getPath().getText()
  or result = owner.(Impl).getSelfTy().toString() and
    not owner.(Impl).getSelfTy() instanceof PathTypeRepr
  or result = owner.(Trait).getName().getText()
}

Item owner(PathTypeRepr t) {
  result = t.getParentNode+() and exists(ownerName(result)) and
  not exists(Item mid |
    mid = t.getParentNode+() and exists(ownerName(mid)) and mid.getParentNode+() = result
  )
}

string syntax(PathTypeRepr t) {
  result = "field" and exists(StructField p | p = t.getParentNode+())
  or result = "parameter" and exists(Param p | p = t.getParentNode+())
  or result = "return" and exists(ReturnTypeSyntax p | p = t.getParentNode+())
  or result = "local" and exists(LetStmt p | p = t.getParentNode+())
  or result = "generic" and exists(GenericArg p | p = t.getParentNode+())
  or result = "alias" and exists(TypeAlias p | p = t.getParentNode+())
  or result = "impl" and exists(Impl p | p = t.getParentNode+())
  or result = "other" and not exists(StructField p | p = t.getParentNode+()) and
    not exists(Param p | p = t.getParentNode+()) and
    not exists(ReturnTypeSyntax p | p = t.getParentNode+()) and
    not exists(LetStmt p | p = t.getParentNode+()) and
    not exists(GenericArg p | p = t.getParentNode+()) and
    not exists(TypeAlias p | p = t.getParentNode+()) and
    not exists(Impl p | p = t.getParentNode+())
}

from PathTypeRepr t, ItemNode target, Item o
where
  t.fromSource() and target.fromSource() and
  target = resolvePath(t.getPath()) and
  (target instanceof Struct or target instanceof Enum or
   target instanceof Trait or target instanceof TypeAlias) and
  not t.isInMacroExpansion() and
  exists(target.getLocation().getFile().getRelativePath()) and
  exists(t.getLocation().getFile().getRelativePath()) and
  o = owner(t) and exists(target.getName())
select t.getLocation().getFile().getRelativePath() as owner_file, ownerName(o) as owner_name,
  target.getLocation().getFile().getRelativePath() as target_file, target.getName() as target_name,
  syntax(t) as syntax
