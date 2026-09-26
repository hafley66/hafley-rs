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
  exists(target.getLocation().getFile().getRelativePath()) and
  exists(t.getLocation().getFile().getRelativePath()) and
  o = owner(t)
select t.getLocation().getFile().getRelativePath() as owner_file, ownerName(o) as owner_name,
  target.getLocation().getFile().getRelativePath() as target_file, target.getName() as target_name
