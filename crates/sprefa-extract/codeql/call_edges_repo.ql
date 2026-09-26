/** Rust explicit calls with the same four-column key as ryi. */
import rust
import codeql.rust.internal.PathResolution

from AstNode site, ItemNode target, Function source
where
  site.fromSource() and target.fromSource() and
  not site.isInMacroExpansion() and
  (target = site.(Call).getStaticTarget() or
   target = site.(StructExpr).getStruct()) and
  source = site.getEnclosingCallable() and
  exists(target.getLocation().getFile().getRelativePath()) and
  exists(site.getLocation().getFile().getRelativePath()) and
  exists(target.getName())
select site.getLocation().getFile().getRelativePath() as src_file,
  source.getName().getText() as enclosing_item,
  target.getLocation().getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
