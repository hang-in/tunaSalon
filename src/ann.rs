//! HNSW ANN 인덱스 래퍼 (usearch, cosine, f32).
//!
//! seCall `crates/secall-core/src/search/ann.rs`에서 lift.
//! 변경: anyhow → String 에러, tracing 제거, `in_memory()` 신규 추가.
//!
//! `friend-engine-semantic` + `cfg(not(windows))` 이중 게이팅.
//! 기본/`friend-engine` 빌드에서는 이 모듈 자체가 컴파일되지 않는다.

#![cfg(all(feature = "friend-engine-semantic", not(target_os = "windows")))]

use std::path::{Path, PathBuf};
use usearch::{new_index, Index, IndexOptions, MetricKind, ScalarKind};

// ─── AnnIndex ────────────────────────────────────────────────────────────────

pub struct AnnIndex {
    index: Index,
    /// None = in-memory (save 불가).
    path: Option<PathBuf>,
    dims: usize,
}

impl AnnIndex {
    /// `path`에 `.usearch` 파일이 있으면 로드, 없으면 새로 생성(reserve 10_000).
    pub fn open_or_create(path: &Path, dims: usize) -> Result<Self, String> {
        let index = make_index(dims)?;

        if path.exists() {
            let path_str = path
                .to_str()
                .ok_or_else(|| format!("non-UTF-8 ANN path: {:?}", path))?;
            index.load(path_str).map_err(|e| format!("ann load: {e}"))?;

            // 기존 capacity 위에 여유분 추가
            let current = index.size();
            let reserve_target = current + 10_000;
            index
                .reserve(reserve_target)
                .map_err(|e| format!("ann reserve(load): {e}"))?;
        } else {
            index
                .reserve(10_000)
                .map_err(|e| format!("ann reserve(new): {e}"))?;
        }

        Ok(Self {
            index,
            path: Some(path.to_path_buf()),
            dims,
        })
    }

    /// 파일 없는 인메모리 인덱스 (`:memory:` store·테스트용).
    /// `save()`를 호출해도 noop.
    pub fn in_memory(dims: usize) -> Result<Self, String> {
        let index = make_index(dims)?;
        index
            .reserve(10_000)
            .map_err(|e| format!("ann in_memory reserve: {e}"))?;
        Ok(Self {
            index,
            path: None,
            dims,
        })
    }

    /// 벡터 추가. capacity 초과 시 자동 reserve.
    pub fn add(&self, key: u64, vec: &[f32]) -> Result<(), String> {
        if self.index.size() >= self.index.capacity() {
            let new_cap = self.index.capacity() + 10_000;
            self.index
                .reserve(new_cap)
                .map_err(|e| format!("ann auto-reserve: {e}"))?;
        }
        self.index
            .add(key, vec)
            .map_err(|e| format!("ann add key={key}: {e}"))
    }

    /// ANN 검색. 상위 `limit`개의 `(key, distance)` 반환.
    /// distance가 낮을수록 코사인 유사도가 높음(가까움).
    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<(u64, f32)>, String> {
        let results = self
            .index
            .search(query, limit)
            .map_err(|e| format!("ann search: {e}"))?;
        Ok(results.keys.into_iter().zip(results.distances).collect())
    }

    /// 인덱스를 파일에 저장. `in_memory()` 생성 인덱스는 noop.
    pub fn save(&self) -> Result<(), String> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()), // in-memory: 저장 불가, noop
        };
        let path_str = path
            .to_str()
            .ok_or_else(|| format!("non-UTF-8 ANN path: {:?}", path))?;
        self.index
            .save(path_str)
            .map_err(|e| format!("ann save: {e}"))
    }

    pub fn size(&self) -> usize {
        self.index.size()
    }

    pub fn dimensions(&self) -> usize {
        self.dims
    }
}

/// 공통 `IndexOptions` 생성 + `new_index` 호출.
fn make_index(dims: usize) -> Result<Index, String> {
    let options = IndexOptions {
        dimensions: dims,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        connectivity: 0,
        expansion_add: 0,
        expansion_search: 0,
        multi: false,
    };
    new_index(&options).map_err(|e| format!("new_index: {e}"))
}

// ─── 단위 테스트 ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// in_memory: add/search 기본 동작.
    #[test]
    fn in_memory_add_search() {
        let ann = AnnIndex::in_memory(4).unwrap();
        assert_eq!(ann.size(), 0);
        assert_eq!(ann.dimensions(), 4);

        ann.add(1, &[1.0_f32, 0.0, 0.0, 0.0]).unwrap();
        ann.add(2, &[0.0_f32, 1.0, 0.0, 0.0]).unwrap();
        ann.add(3, &[0.0_f32, 0.0, 1.0, 0.0]).unwrap();
        assert_eq!(ann.size(), 3);

        // 쿼리 [1,0,0,0]과 가장 가까운 벡터 = key 1
        let results = ann.search(&[1.0_f32, 0.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1, "key 1이 nearest여야 한다");
        assert!(results[0].1 < 0.01, "distance가 거의 0이어야 한다 (동일 벡터)");
    }

    /// in_memory: search 상위 k, distance 기준 오름차순.
    #[test]
    fn in_memory_search_top_k_ordering() {
        let ann = AnnIndex::in_memory(3).unwrap();
        // 명확히 분리된 벡터들
        ann.add(10, &[1.0_f32, 0.0, 0.0]).unwrap(); // 쿼리와 동일
        ann.add(20, &[0.0_f32, 1.0, 0.0]).unwrap(); // 직교
        ann.add(30, &[0.0_f32, 0.0, 1.0]).unwrap(); // 직교

        let results = ann.search(&[1.0_f32, 0.1, 0.0], 3).unwrap();
        assert_eq!(results.len(), 3);
        // 첫 번째 결과가 key=10 (가장 가까움)
        assert_eq!(results[0].0, 10, "key 10이 nearest여야 한다");
    }

    /// in_memory save = noop (패닉 없이 Ok 반환).
    #[test]
    fn in_memory_save_is_noop() {
        let ann = AnnIndex::in_memory(4).unwrap();
        ann.add(1, &[1.0_f32, 0.0, 0.0, 0.0]).unwrap();
        ann.save().unwrap(); // should not panic
    }

    /// open_or_create: add + search 기본 동작.
    #[test]
    fn open_or_create_add_search() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.usearch");

        let ann = AnnIndex::open_or_create(&path, 3).unwrap();
        assert_eq!(ann.size(), 0);

        ann.add(1, &[1.0_f32, 0.0, 0.0]).unwrap();
        ann.add(2, &[0.0_f32, 1.0, 0.0]).unwrap();
        assert_eq!(ann.size(), 2);

        let results = ann.search(&[1.0_f32, 0.1, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1, "key 1이 nearest여야 한다");
    }

    /// save → load roundtrip: 저장 후 재로드해도 동일 벡터를 검색.
    #[test]
    fn save_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("roundtrip.usearch");

        {
            let ann = AnnIndex::open_or_create(&path, 3).unwrap();
            ann.add(42, &[1.0_f32, 0.0, 0.0]).unwrap();
            ann.add(99, &[0.0_f32, 1.0, 0.0]).unwrap();
            ann.save().unwrap();
        }
        // 파일 존재 확인
        assert!(path.exists(), "save 후 파일이 존재해야 한다");

        // 재로드
        let ann2 = AnnIndex::open_or_create(&path, 3).unwrap();
        assert_eq!(ann2.size(), 2, "재로드 후 size==2여야 한다");

        let results = ann2.search(&[1.0_f32, 0.0, 0.0], 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 42, "재로드 후 key 42가 nearest여야 한다");
    }
}
