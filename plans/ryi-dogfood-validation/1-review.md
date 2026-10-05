# Defect 1 review

The free-function walk entry was removed by naming the traversal retain_module_ancestry.

The fs_read_without_io_path rule matches only fs::read callees, regardless of their arguments. Both retained entries are false positives under that rule:

- context::load reads with `std::fs::read(crate::read::io_path(Path::new(&path)))`.
- context::retain_module_ancestry reads with `std::fs::read(crate::read::io_path(Path::new(path)))`.

Both paths pass through the request-local io_path normalization. There is no unnormalized read in either entry.
