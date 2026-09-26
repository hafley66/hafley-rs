import rust
import codeql.rust.internal.PathResolution

string ownerName(Item owner) {
  result = owner.(Struct).getName().getText()
  or
  result = owner.(Enum).getName().getText()
  or
  result = owner.(Function).getName().getText()
  or
  result = owner.(TypeAlias).getName().getText()
  or
  result = owner.(Impl).getSelfTy().(PathTypeRepr).getPath().getText()
  or
  result = owner.(Impl).getSelfTy().toString() and
    not owner.(Impl).getSelfTy() instanceof PathTypeRepr
  or
  result = owner.(Trait).getName().getText()
}

Item owner(PathTypeRepr t) {
  result = t.getParentNode+() and
  exists(ownerName(result)) and
  not exists(Item mid |
    mid = t.getParentNode+() and exists(ownerName(mid)) and mid.getParentNode+() = result
  )
}

from PathTypeRepr t, ItemNode target, Item o
where
  target = resolvePath(t.getPath()) and
  not t.isInMacroExpansion() and
  (target instanceof Struct or target instanceof Enum or
   target instanceof Trait or target instanceof TypeAlias) and
  target.getLocation().getFile().getBaseName() = "_0_types.rs" and
  o = owner(t)
select t.getLocation().getFile().getRelativePath() as src_file,
  ownerName(o) as enclosing_item,
  target.getLocation().getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
