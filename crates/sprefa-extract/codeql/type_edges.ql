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
  result = owner.(Impl).getSelfTy().toString()
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
  target.getLocation().getFile().getBaseName() = "_0_types.rs" and
  o = owner(t)
select t.getLocation().getFile().getBaseName() as file_name, ownerName(o) as owner_name,
  target.getName() as target_name, t.getLocation().getStartLine() as line
