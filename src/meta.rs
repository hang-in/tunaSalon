//! MetaController: FlowMeter 수렴도 → μ 식히기 비율(mu_scale) 계산.
//!
//! 순수·결정적·유계. 배선(driver/live 통합)은 task-37.
//! `cooling` 반환값은 항상 `[floor, 1.0]`.

/// 대화 수렴 시 발화 강도(μ)를 얼마나 낮출지 결정하는 휴리스틱.
///
/// - `gain`: 식히기 강도. 클수록 수렴 시 mu_scale 더 낮춤.
/// - `threshold`: 이 수렴도를 초과해야 식히기 시작.
/// - `floor`: mu_scale 하한. 완전 침묵/고착 방지.
#[derive(Debug, Clone, Copy)]
pub struct MetaController {
    pub gain: f64,
    pub threshold: f64,
    pub floor: f64,
}

impl MetaController {
    /// 직접 파라미터로 생성한다.
    pub fn new(gain: f64, threshold: f64, floor: f64) -> Self {
        Self {
            gain,
            threshold,
            floor,
        }
    }

    /// 환경 변수에서 gain을 읽어 기본값을 덮어쓴다.
    ///
    /// `SALON_META_GAIN`이 유효한 f64면 `[0.0, 1.0]`으로 clamp 후 gain으로 사용.
    /// threshold·floor는 기본값 그대로.
    pub fn from_env() -> Self {
        let mut ctrl = Self::default();
        if let Ok(raw) = std::env::var("SALON_META_GAIN") {
            if let Ok(val) = raw.parse::<f64>() {
                ctrl.gain = val.clamp(0.0, 1.0);
            }
        }
        ctrl
    }

    /// FlowMetric 수렴도를 받아 mu_scale ∈ [floor, 1.0]을 반환한다.
    ///
    /// - `flow == None` → **1.0** (콘텐츠 없음, 골든 보존).
    /// - `conv <= threshold` → 1.0 (아직 식힐 만큼 수렴 안 됨).
    /// - else: `overshoot = (conv - threshold) / (1 - threshold)` ∈ [0,1],
    ///   `scale = (1.0 - gain * overshoot).clamp(floor, 1.0)`.
    /// - `threshold == 1.0`: 분모가 0이므로 항상 1.0(0-division 방어).
    /// - `gain == 0.0`: overshoot와 무관히 항상 1.0(비활성).
    pub fn cooling(&self, flow: Option<crate::flow::FlowMetric>) -> f64 {
        // content 없음 → no-op
        let Some(fm) = flow else {
            return 1.0;
        };

        let conv = fm.convergence;

        // 아직 threshold 미초과 → 식히기 없음
        if conv <= self.threshold {
            return 1.0;
        }

        // threshold == 1.0 이면 분모가 0 → 방어
        let denom = 1.0 - self.threshold;
        if denom <= 0.0 {
            return 1.0;
        }

        // 정규화 overshoot ∈ [0, 1]
        let overshoot = ((conv - self.threshold) / denom).clamp(0.0, 1.0);

        // mu_scale: gain=0 이면 1.0, gain 클수록 낮아짐
        let scale = 1.0 - self.gain * overshoot;
        scale.clamp(self.floor, 1.0)
    }
}

impl Default for MetaController {
    /// 약한 기본값 — 진동/고착 방지.
    ///
    /// `gain: 0.6`, `threshold: 0.5`, `floor: 0.4`.
    fn default() -> Self {
        Self {
            gain: 0.6,
            threshold: 0.5,
            floor: 0.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::FlowMetric;

    fn fm(convergence: f64) -> Option<FlowMetric> {
        Some(FlowMetric { convergence })
    }

    /// (1) flow == None → 1.0 (어떤 controller에서도).
    #[test]
    fn none_flow_returns_one() {
        let ctrl = MetaController::default();
        assert_eq!(ctrl.cooling(None), 1.0);

        let ctrl2 = MetaController::new(1.0, 0.0, 0.1);
        assert_eq!(ctrl2.cooling(None), 1.0);
    }

    /// (2) conv ≤ threshold → 1.0 (기본값 컨트롤러, conv=0.3).
    #[test]
    fn below_threshold_returns_one() {
        let ctrl = MetaController::default(); // threshold=0.5
        assert_eq!(ctrl.cooling(fm(0.3)), 1.0);
        assert_eq!(ctrl.cooling(fm(0.5)), 1.0); // 경계값(≤)
    }

    /// (3) 단조: conv 높아질수록 mu_scale 낮아지거나 같음.
    #[test]
    fn monotone_decreasing() {
        let ctrl = MetaController::default();
        let s06 = ctrl.cooling(fm(0.6));
        let s08 = ctrl.cooling(fm(0.8));
        let s10 = ctrl.cooling(fm(1.0));
        assert!(s06 >= s08, "cooling(0.6) >= cooling(0.8)");
        assert!(s08 >= s10, "cooling(0.8) >= cooling(1.0)");
    }

    /// (4) conv=1.0 에서 mu_scale ≥ floor (하한 보장).
    ///     기본값(gain=0.6): scale = 1-0.6 = 0.4 = floor.
    #[test]
    fn floor_is_respected() {
        let ctrl = MetaController::default();
        let scale = ctrl.cooling(fm(1.0));
        assert!(
            scale >= ctrl.floor,
            "mu_scale({scale}) ≥ floor({f})",
            f = ctrl.floor
        );
    }

    /// (5) gain == 0 → 항상 1.0 (비활성).
    #[test]
    fn zero_gain_always_one() {
        let ctrl = MetaController::new(0.0, 0.5, 0.4);
        assert_eq!(ctrl.cooling(fm(0.0)), 1.0);
        assert_eq!(ctrl.cooling(fm(0.75)), 1.0);
        assert_eq!(ctrl.cooling(fm(1.0)), 1.0);
        assert_eq!(ctrl.cooling(None), 1.0);
    }

    /// (6) 손계산: gain=0.6, threshold=0.5, conv=0.75
    ///     overshoot = (0.75-0.5)/0.5 = 0.5
    ///     scale = 1 - 0.6*0.5 = 0.7
    #[test]
    fn hand_computed_value() {
        let ctrl = MetaController::new(0.6, 0.5, 0.4);
        let scale = ctrl.cooling(fm(0.75));
        let expected = 0.7_f64;
        assert!((scale - expected).abs() < 1e-9, "scale({scale}) ≈ 0.7 기대");
    }

    /// (7) threshold == 1.0 → 분모 0 방어 → 항상 1.0.
    #[test]
    fn threshold_one_guard() {
        let ctrl = MetaController::new(0.6, 1.0, 0.4);
        // conv=1.0 은 threshold=1.0 과 같으므로 conv <= threshold → 1.0
        assert_eq!(ctrl.cooling(fm(1.0)), 1.0);
        // conv < threshold 도 당연히 1.0
        assert_eq!(ctrl.cooling(fm(0.9)), 1.0);
    }
}
