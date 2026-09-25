use std::collections::BTreeMap;
use std::mem::size_of;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{
    BytePosition, ReadRequest, SourceBytes, SourceSpan, SpanPosition, SpanPositionRequest,
    SpanText, SpanTextRequest,
};
use crate::_7_source_tree::SourceTree;

/// Newline starts for one retrieved source buffer. The caller-provided budget
/// is checked before allocating the index. The index is retrieval-local and
/// is never persisted as span storage.
struct NewlineIndex {
    starts: Vec<usize>,
}

#[derive(Clone, Copy)]
struct NewlineIndexStorage {
    line_start_count: usize,
    bytes: u64,
}

impl NewlineIndex {
    fn storage(bytes: &[u8]) -> Result<NewlineIndexStorage> {
        let line_start_count = bytes
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            .checked_add(1)
            .context("newline count exceeds usize")?;
        let storage_bytes = line_start_count
            .checked_mul(size_of::<usize>())
            .context("newline index storage exceeds usize")?;
        let storage_bytes_u64 =
            u64::try_from(storage_bytes).context("newline index storage exceeds u64")?;
        Ok(NewlineIndexStorage {
            line_start_count,
            bytes: storage_bytes_u64,
        })
    }

    fn check_budget(storage: NewlineIndexStorage, byte_budget: u64) -> Result<()> {
        if storage.bytes > byte_budget {
            bail!(
                "newline index needs {} bytes for {} line starts, exceeding budget {byte_budget}",
                storage.bytes,
                storage.line_start_count,
            );
        }
        Ok(())
    }

    fn build(bytes: &[u8], storage: NewlineIndexStorage) -> Result<Self> {
        let mut starts = Vec::new();
        starts
            .try_reserve_exact(storage.line_start_count)
            .context("allocate newline index")?;
        starts.push(0);
        for (offset, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Ok(Self { starts })
    }

    fn position(&self, offset: usize) -> Result<BytePosition> {
        let line = self.starts.partition_point(|start| *start <= offset);
        let line = line
            .checked_sub(1)
            .context("newline index has no source start")?;
        let line_start = self.starts[line];
        Ok(BytePosition {
            line: u64::try_from(line.checked_add(1).context("line index exceeds usize")?)
                .context("line index exceeds u64")?,
            byte_column: u64::try_from(offset - line_start).context("byte column exceeds u64")?,
        })
    }
}

fn slice<'a>(span: &SourceSpan, bytes: &'a [u8]) -> Result<&'a [u8]> {
    let length = u64::try_from(bytes.len()).context("source byte length exceeds u64")?;
    if span.start > span.end {
        bail!(
            "span start {} exceeds end {} for {}",
            span.start,
            span.end,
            span.source.path.0
        );
    }
    if span.end > length {
        bail!(
            "span end {} exceeds source length {} for {}",
            span.end,
            length,
            span.source.path.0
        );
    }
    let start = usize::try_from(span.start).context("span start exceeds usize")?;
    let end = usize::try_from(span.end).context("span end exceeds usize")?;
    Ok(&bytes[start..end])
}

/// Narrow a source request batch to unique `(SourceRef, expected ContentId)`
/// pairs. The returned indices restore the original request order.
fn deduplicate_read_requests(
    requests: impl IntoIterator<Item = ReadRequest>,
) -> (Vec<ReadRequest>, Vec<usize>) {
    let mut indexes = BTreeMap::new();
    let mut unique = Vec::new();
    let mut order = Vec::new();
    for request in requests {
        let key = (request.source.clone(), request.expected.clone());
        let next = unique.len();
        let index = *indexes.entry(key).or_insert_with(|| {
            unique.push(request);
            next
        });
        order.push(index);
    }
    (unique, order)
}

struct SpanReadBatch {
    unique: Vec<SourceBytes>,
    order: Vec<usize>,
}

impl SpanReadBatch {
    fn source(&self, request_index: usize) -> Result<&SourceBytes> {
        let source_index = *self
            .order
            .get(request_index)
            .context("span request has no deduplicated read index")?;
        self.unique
            .get(source_index)
            .context("deduplicated read response is missing")
    }
}

fn read_for_spans(
    tree: &mut SourceTree,
    requests: impl IntoIterator<Item = ReadRequest>,
) -> Result<SpanReadBatch> {
    let (unique_requests, order) = deduplicate_read_requests(requests);
    let unique_sources = tree.read_many(&unique_requests)?;
    Ok(SpanReadBatch {
        unique: unique_sources,
        order,
    })
}

/// Validate every position request against the storage shared by its unique
/// source read, then build exactly one index per unique read response.
fn build_newline_indexes(
    batch: &SpanReadBatch,
    requests: &[SpanPositionRequest],
) -> Result<Vec<NewlineIndex>> {
    let storage: Vec<_> = batch
        .unique
        .iter()
        .map(|source| NewlineIndex::storage(&source.bytes))
        .collect::<Result<_>>()?;
    for (request_index, request) in requests.iter().enumerate() {
        let source_index = *batch
            .order
            .get(request_index)
            .context("span request has no deduplicated read index")?;
        let source = batch.source(request_index)?;
        slice(&request.span, &source.bytes)?;
        let requirement = *storage
            .get(source_index)
            .context("newline storage is missing for deduplicated read")?;
        NewlineIndex::check_budget(requirement, request.newline_index_byte_budget)?;
    }
    batch
        .unique
        .iter()
        .zip(storage)
        .map(|(source, storage)| NewlineIndex::build(&source.bytes, storage))
        .collect()
}

/// Create a child span inside `parent` from child-relative half-open byte
/// offsets. The child retains the parent's exact `SourceRef`; no source bytes
/// or relational row IDs are copied or allocated.
pub fn span_slice(
    parent: &SourceSpan,
    relative_start: u64,
    relative_end: u64,
) -> Result<SourceSpan> {
    let parent_length = parent
        .end
        .checked_sub(parent.start)
        .context("parent span start exceeds end")?;
    if relative_start > relative_end {
        bail!("child span start {relative_start} exceeds end {relative_end}");
    }
    if relative_end > parent_length {
        bail!("child span end {relative_end} exceeds parent length {parent_length}");
    }
    Ok(SourceSpan {
        source: parent.source.clone(),
        start: parent
            .start
            .checked_add(relative_start)
            .context("child span start exceeds u64")?,
        end: parent
            .start
            .checked_add(relative_end)
            .context("child span end exceeds u64")?,
    })
}

impl SourceTree {
    /// Retrieve exact bytes for several revision-qualified spans in one source
    /// batch. Commit requests share the tree's persistent `GitBatch` reader.
    pub fn span_text_many(&mut self, requests: &[SpanTextRequest]) -> Result<Vec<SpanText>> {
        let batch = read_for_spans(
            self,
            requests.iter().map(|request| ReadRequest {
                source: request.span.source.clone(),
                expected: request.expected.clone(),
            }),
        )?;
        requests
            .iter()
            .enumerate()
            .map(|(request_index, request)| {
                let source = batch.source(request_index)?;
                let bytes = Arc::from(slice(&request.span, &source.bytes)?.to_vec());
                Ok(SpanText {
                    span: request.span.clone(),
                    content: source.content.clone(),
                    bytes,
                })
            })
            .collect()
    }

    /// Resolve start and exclusive-end byte positions for several spans in one
    /// source batch. Each request imposes its own explicit newline-index budget.
    pub fn span_position_many(
        &mut self,
        requests: &[SpanPositionRequest],
    ) -> Result<Vec<SpanPosition>> {
        let batch = read_for_spans(
            self,
            requests.iter().map(|request| ReadRequest {
                source: request.span.source.clone(),
                expected: request.expected.clone(),
            }),
        )?;
        let indexes = build_newline_indexes(&batch, requests)?;
        requests
            .iter()
            .enumerate()
            .map(|(request_index, request)| {
                let source_index = *batch
                    .order
                    .get(request_index)
                    .context("span request has no deduplicated read index")?;
                let source = batch.source(request_index)?;
                let index = indexes
                    .get(source_index)
                    .context("newline index is missing for deduplicated read")?;
                let start =
                    usize::try_from(request.span.start).context("span start exceeds usize")?;
                let end = usize::try_from(request.span.end).context("span end exceeds usize")?;
                Ok(SpanPosition {
                    span: request.span.clone(),
                    content: source.content.clone(),
                    start: index.position(start)?,
                    end: index.position(end)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::_0_types::{
        ContentId, ObjectId, RepoPath, RepositoryId, RevisionId, SourceBytes, SourceRef,
        SourceSpan, SpanPositionRequest,
    };

    use super::{build_newline_indexes, deduplicate_read_requests, ReadRequest, SpanReadBatch};

    fn source_ref() -> SourceRef {
        SourceRef {
            repository: RepositoryId(Arc::from("repository")),
            revision: RevisionId::Commit(ObjectId(Arc::from("commit"))),
            path: RepoPath(Arc::from("src/lib.rs")),
        }
    }

    #[test]
    fn deduplicate_read_requests_keeps_one_read_per_source_and_expectation() {
        let source = source_ref();
        let expected = Some(ContentId::GitBlob(ObjectId(Arc::from("blob"))));
        let (unique, order) = deduplicate_read_requests([
            ReadRequest {
                source: source.clone(),
                expected: expected.clone(),
            },
            ReadRequest { source, expected },
        ]);
        assert_eq!(unique.len(), 1);
        assert_eq!(order, vec![0, 0]);
    }

    #[test]
    fn multiple_spans_share_one_newline_index_and_each_budget_is_checked() {
        let source = source_ref();
        let content = ContentId::GitBlob(ObjectId(Arc::from("blob")));
        let batch = SpanReadBatch {
            unique: vec![SourceBytes {
                source: source.clone(),
                content: content.clone(),
                bytes: Arc::from(&b"first\nsecond\n"[..]),
            }],
            order: vec![0, 0],
        };
        let storage = u64::try_from(3 * std::mem::size_of::<usize>()).unwrap();
        let requests = [
            SpanPositionRequest {
                span: SourceSpan {
                    source: source.clone(),
                    start: 0,
                    end: 5,
                },
                expected: Some(content.clone()),
                newline_index_byte_budget: storage,
            },
            SpanPositionRequest {
                span: SourceSpan {
                    source: source.clone(),
                    start: 6,
                    end: 12,
                },
                expected: Some(content),
                newline_index_byte_budget: storage,
            },
        ];
        let indexes = build_newline_indexes(&batch, &requests).unwrap();
        assert_eq!(indexes.len(), 1);

        let mut under_budget = requests[1].clone();
        under_budget.newline_index_byte_budget -= 1;
        assert!(build_newline_indexes(&batch, &[requests[0].clone(), under_budget]).is_err());
    }
}
