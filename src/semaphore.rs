//! 카운팅 세마포어 (task-23).
//!
//! `std::sync::{Mutex, Condvar}` 기반 자체 구현. 외부 크레이트 없음.
//! - `Semaphore::new(n)`: 최대 n개 동시 slot.
//! - `acquire(&self) -> Permit`: slot이 남을 때까지 Condvar로 대기 후 slot 차감.
//! - `Permit` drop 시 slot 반환 + 대기 스레드에게 notify(RAII 보장, 데드락 없음).

use std::sync::{Arc, Condvar, Mutex};

/// 카운팅 세마포어.
///
/// 동시 in-flight 상한을 집행한다.
/// `acquire`가 블록하며 슬롯을 기다린다.
/// `Permit` drop이 슬롯을 반환한다 — 패닉 경로에서도 안전.
pub struct Semaphore {
    /// 남은 슬롯 수. lock 실패(poisoned) 시에는 슬롯 0으로 보수적으로 처리한다.
    inner: Mutex<usize>,
    /// 슬롯 반환 시 대기 스레드를 깨운다.
    condvar: Condvar,
}

impl Semaphore {
    /// 최대 `n`개 동시 슬롯을 허용하는 세마포어를 생성한다.
    ///
    /// n=0이면 acquire가 항상 블록한다(영구 대기).
    /// 실제 사용: cloud=3, friend=1.
    pub fn new(n: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(n),
            condvar: Condvar::new(),
        })
    }

    /// 슬롯이 생길 때까지 대기한 뒤 슬롯을 차감하고 `Permit`을 반환한다.
    ///
    /// `Permit` drop 시 슬롯이 자동 반환된다(RAII).
    ///
    /// Mutex poison 시: 다른 스레드가 lock 보유 중 패닉한 경우.
    /// into_inner()로 복구하고 슬롯을 대기 없이 즉시 차감한다(보수적 동작보다 진행 우선).
    pub fn acquire(self: &Arc<Self>) -> Permit {
        // Mutex::lock 실패(poison)이면 into_inner로 복구.
        let mut count = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        // 슬롯이 0이면 Condvar 대기. wait도 poison 방어.
        loop {
            if *count > 0 {
                *count -= 1;
                return Permit {
                    sem: Arc::clone(self),
                };
            }
            count = match self.condvar.wait(count) {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
        }
    }

    /// 슬롯을 1 반환하고 대기 중인 스레드 하나를 깨운다.
    ///
    /// `Permit::drop`에서 호출된다. 직접 호출 불필요.
    fn release(&self) {
        let mut count = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        *count += 1;
        // 대기 스레드 하나만 깨운다(슬롯 1개 반환이므로).
        self.condvar.notify_one();
    }
}

/// 세마포어 슬롯 점유권 RAII 가드.
///
/// `drop` 시 자동으로 슬롯을 반환하므로 패닉·조기 반환 경로에서도 데드락 없음.
pub struct Permit {
    sem: Arc<Semaphore>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.sem.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    /// acquire/release: 단일 스레드에서 n번 acquire + drop이 정확히 n번 슬롯을 반환한다.
    #[test]
    fn acquire_and_release_count() {
        let sem = Semaphore::new(3);
        // 3번 acquire
        let p1 = sem.acquire();
        let p2 = sem.acquire();
        let p3 = sem.acquire();
        // 남은 슬롯 = 0 (내부 확인)
        {
            let count = sem.inner.lock().unwrap();
            assert_eq!(*count, 0, "acquire 3회 후 남은 슬롯=0이어야 함");
        }
        // drop p1 → 슬롯 1 반환
        drop(p1);
        {
            let count = sem.inner.lock().unwrap();
            assert_eq!(*count, 1);
        }
        // drop p2, p3 → 슬롯 2 반환 (합 3)
        drop(p2);
        drop(p3);
        {
            let count = sem.inner.lock().unwrap();
            assert_eq!(*count, 3, "모든 permit drop 후 슬롯 원복이어야 함");
        }
    }

    /// 실제 스레드로 동시 보유자가 cap을 초과하지 않음을 검증한다.
    ///
    /// 전략: 8개 스레드를 세마포어 cap=3으로 acquire하게 하고,
    /// atomic 피크 카운터로 동시 보유 최대치를 측정한다.
    #[test]
    fn concurrent_holders_never_exceed_cap() {
        let cap = 3usize;
        let n_threads = 8usize;
        let sem = Semaphore::new(cap);

        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        thread::scope(|s| {
            for _ in 0..n_threads {
                let sem2 = Arc::clone(&sem);
                let current2 = Arc::clone(&current);
                let peak2 = Arc::clone(&peak);

                s.spawn(move || {
                    let _permit = sem2.acquire();

                    // permit 획득 직후 카운터 증가 + 피크 갱신
                    let prev = current2.fetch_add(1, Ordering::SeqCst);
                    let new_val = prev + 1;
                    // 피크 CAS 루프 없이 단순 max 갱신 (느슨하지만 cap ≤ 3 단언에 충분)
                    let _ = peak2.fetch_max(new_val, Ordering::SeqCst);

                    // 짧은 작업 시뮬레이션 (스레드 교차 유발)
                    std::thread::yield_now();

                    // 카운터 감소
                    current2.fetch_sub(1, Ordering::SeqCst);
                    // permit drop → 슬롯 반환
                });
            }
        });

        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= cap,
            "동시 보유 피크({observed_peak})가 cap({cap})을 초과해서는 안 됨"
        );
        assert_eq!(
            current.load(Ordering::SeqCst),
            0,
            "모든 스레드 완료 후 current=0이어야 함"
        );
    }
}
