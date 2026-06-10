//! 翻页核心状态机(M1-T2;D2 冻结签名,R-009)
//!
//! 纯逻辑、与 async I/O 解耦(FR-001 ②)。语义样板 = facebook.rs:522-644 既有循环
//! (M1 不改 facebook.rs;M2 接通)。
//!
//! 冻结语义(见 plans/modules/m1-pagination-core.md §2 D2):
//! - **DR-03 `empty_streak`**:按 `newly_accepted.is_empty()` 递增(非按原始 item_ids 长度)
//!   ——重复内容页连发与字面空页同等计入(钉子测试 = 单测 7)。
//! - **SM#F8 同页多信号优先序**:`ReachedMaxCount` 优先于一切枯竭信号(钉子测试 = 单测 8)。

use crate::ports::content_gateway::FetchShortfall;
use std::collections::HashSet;

/// 连续空进展页上限(对齐 facebook.rs:28 MAX_EMPTY_CURSOR_HOPS)
pub const MAX_EMPTY_PAGES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// 达量
    ReachedMaxCount,
    /// next_cursor 缺失(适配器把 has_more=false 归一化为 None)
    UpstreamExhausted,
    /// 重复 cursor(F-004)
    CursorLoop,
    /// 连续空页达上限(F-005)
    EmptyPageLimit,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageDecision {
    Continue { cursor: String },
    Stop(StopReason),
}

pub struct PaginationLoop {
    max_count: usize,
    seen_ids: HashSet<String>,
    seen_cursors: HashSet<String>,
    accepted: usize,
    empty_streak: u32,
}

pub struct PageOutcome {
    pub newly_accepted: Vec<String>,
    pub decision: PageDecision,
}

impl PaginationLoop {
    /// 调用方须保证 `max_count > 0`:传 0 时首次 `accept_page` 即 `Stop(ReachedMaxCount)`
    /// 且 `shortfall_for` 返回 `None`(静默空完成)——生产路径(redis 映射缺省 10)不会产生 0,
    /// 但 M2~M5 消费方不得依赖 0 的行为(M1-T2 质量审观察项)。
    pub fn new(max_count: usize) -> Self {
        Self {
            max_count,
            seen_ids: HashSet::new(),
            seen_cursors: HashSet::new(),
            accepted: 0,
            empty_streak: 0,
        }
    }

    /// 喂入一页(条目 id 列表 + 下一页 cursor),返回「新接受的 id 子集」与决策。
    /// 去重(I-003):seen id 不重复接受;接受数严格 ≤ max_count(I-001,页内截断)。
    ///
    /// 信号判定顺序(D2 冻结):
    /// 1. 接受本页新条目(去重 + 截断到剩余额度);
    /// 2. 达量 → `Stop(ReachedMaxCount)`(SM#F8:优先于一切枯竭信号);
    /// 3. 空进展计数/复位(DR-03:按 `newly_accepted.is_empty()`);
    /// 4. 连续空进展达 `MAX_EMPTY_PAGES` → `Stop(EmptyPageLimit)`;
    /// 5. `next_cursor=None` → `Stop(UpstreamExhausted)`;
    /// 6. 重复 cursor → `Stop(CursorLoop)`;
    /// 7. 否则 `Continue { cursor }`。
    pub fn accept_page(&mut self, item_ids: &[String], next_cursor: Option<String>) -> PageOutcome {
        // 1. 去重接受 + 页内截断(I-001/I-003)
        let mut newly_accepted = Vec::new();
        for id in item_ids {
            if self.accepted >= self.max_count {
                break;
            }
            if self.seen_ids.insert(id.clone()) {
                self.accepted += 1;
                newly_accepted.push(id.clone());
            }
        }

        // 2. 达量优先(SM#F8 冻结:即使同页 cursor 缺失/重复/空也取 ReachedMaxCount)
        if self.accepted >= self.max_count {
            return PageOutcome {
                newly_accepted,
                decision: PageDecision::Stop(StopReason::ReachedMaxCount),
            };
        }

        // 3. 空进展计数(DR-03:重复内容页与字面空页同等计入)
        if newly_accepted.is_empty() {
            self.empty_streak += 1;
        } else {
            self.empty_streak = 0;
        }

        // 4. 连续空进展上限(F-005)
        if self.empty_streak >= MAX_EMPTY_PAGES {
            return PageOutcome {
                newly_accepted,
                decision: PageDecision::Stop(StopReason::EmptyPageLimit),
            };
        }

        // 5./6./7. cursor 信号(F-003/F-004)
        let decision = match next_cursor {
            None => PageDecision::Stop(StopReason::UpstreamExhausted),
            Some(cursor) => {
                if self.seen_cursors.insert(cursor.clone()) {
                    PageDecision::Continue { cursor }
                } else {
                    PageDecision::Stop(StopReason::CursorLoop)
                }
            }
        };
        PageOutcome {
            newly_accepted,
            decision,
        }
    }

    pub fn accepted_count(&self) -> usize {
        self.accepted
    }

    /// 终止原因 → 欠交付原因映射(I-004 接线;达量→None;枯竭族 且 accepted<max → Exhausted)
    pub fn shortfall_for(&self, stop: &StopReason) -> Option<FetchShortfall> {
        match stop {
            StopReason::ReachedMaxCount => None,
            StopReason::UpstreamExhausted | StopReason::CursorLoop | StopReason::EmptyPageLimit => {
                if self.accepted < self.max_count {
                    Some(FetchShortfall::Exhausted)
                } else {
                    // 防御:枯竭族但已达量(理论不可达,SM#F8 下达量必为 ReachedMaxCount)
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// 生成 n 个带前缀的唯一 id
    fn ids(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    // ──────────────────────────────────────────────────────────────────
    // 确定性单测 1~8(断言即契约,m1-pagination-core.md §3 M1-T2)
    // ──────────────────────────────────────────────────────────────────

    /// 单测 1(I-001):max=50,3 页 20+20+20 无重复 → 第 3 页截断接受 10、达量停。
    #[test]
    fn reaches_max_count_and_stops() {
        let mut lp = PaginationLoop::new(50);
        let p1 = lp.accept_page(&ids("a", 20), Some("c1".into()));
        assert_eq!(p1.newly_accepted.len(), 20);
        assert_eq!(
            p1.decision,
            PageDecision::Continue {
                cursor: "c1".into()
            }
        );
        let p2 = lp.accept_page(&ids("b", 20), Some("c2".into()));
        assert_eq!(p2.newly_accepted.len(), 20);
        let p3 = lp.accept_page(&ids("c", 20), Some("c3".into()));
        assert_eq!(p3.newly_accepted.len(), 10);
        assert_eq!(lp.accepted_count(), 50);
        assert_eq!(p3.decision, PageDecision::Stop(StopReason::ReachedMaxCount));
        assert_eq!(lp.shortfall_for(&StopReason::ReachedMaxCount), None);
    }

    /// 单测 2(F-003):第 2 页 next_cursor=None → UpstreamExhausted + Exhausted。
    #[test]
    fn upstream_exhausted_when_cursor_missing() {
        let mut lp = PaginationLoop::new(50);
        lp.accept_page(&ids("a", 20), Some("c1".into()));
        let p2 = lp.accept_page(&ids("b", 10), None);
        assert_eq!(
            p2.decision,
            PageDecision::Stop(StopReason::UpstreamExhausted)
        );
        assert_eq!(
            lp.shortfall_for(&StopReason::UpstreamExhausted),
            Some(FetchShortfall::Exhausted)
        );
        assert_eq!(lp.accepted_count(), 30);
    }

    /// 单测 3(F-004):第 2 页返回与第 1 页相同 cursor → CursorLoop(按枯竭语义)。
    #[test]
    fn cursor_loop_detected() {
        let mut lp = PaginationLoop::new(50);
        lp.accept_page(&ids("a", 20), Some("c1".into()));
        let p2 = lp.accept_page(&ids("b", 10), Some("c1".into()));
        assert_eq!(p2.decision, PageDecision::Stop(StopReason::CursorLoop));
        assert_eq!(
            lp.shortfall_for(&StopReason::CursorLoop),
            Some(FetchShortfall::Exhausted)
        );
    }

    /// 单测 4(F-005):连续 3 空页(cursor 各异)→ 第 3 空页 Stop(EmptyPageLimit);
    /// 中间插入非空页则计数复位(第 4 页空不触发)。
    #[test]
    fn empty_page_limit() {
        // 形状 A:3 连空页触发
        let mut lp = PaginationLoop::new(50);
        let e1 = lp.accept_page(&[], Some("e1".into()));
        assert!(matches!(e1.decision, PageDecision::Continue { .. }));
        let e2 = lp.accept_page(&[], Some("e2".into()));
        assert!(matches!(e2.decision, PageDecision::Continue { .. }));
        let e3 = lp.accept_page(&[], Some("e3".into()));
        assert_eq!(e3.decision, PageDecision::Stop(StopReason::EmptyPageLimit));

        // 形状 B:空空 → 非空(复位)→ 第 4 页空不触发
        let mut lp = PaginationLoop::new(50);
        lp.accept_page(&[], Some("e1".into()));
        lp.accept_page(&[], Some("e2".into()));
        let mid = lp.accept_page(&ids("x", 5), Some("e3".into()));
        assert!(matches!(mid.decision, PageDecision::Continue { .. }));
        let e4 = lp.accept_page(&[], Some("e4".into()));
        assert!(matches!(e4.decision, PageDecision::Continue { .. }));
    }

    /// 单测 5(I-003):两页相同 id 集 → 第 2 页 newly_accepted 为空、accepted_count 不变。
    #[test]
    fn duplicate_ids_not_double_counted() {
        let mut lp = PaginationLoop::new(50);
        let p1 = lp.accept_page(&ids("a", 10), Some("c1".into()));
        assert_eq!(p1.newly_accepted.len(), 10);
        assert_eq!(lp.accepted_count(), 10);
        let p2 = lp.accept_page(&ids("a", 10), Some("c2".into()));
        assert!(p2.newly_accepted.is_empty());
        assert_eq!(lp.accepted_count(), 10);
    }

    /// 单测 6(I-004):shortfall_for 完整真值表(欠量态下):
    /// ReachedMaxCount→None;三枯竭族 且 accepted<max → Some(Exhausted)。
    /// (StopReason 枚举不含 PartialFailure 类;适配器错误路径映射归 M1-T3。)
    #[test]
    fn partial_failure_mapping() {
        let mut lp = PaginationLoop::new(50);
        lp.accept_page(&ids("a", 10), Some("c1".into()));
        assert_eq!(lp.shortfall_for(&StopReason::ReachedMaxCount), None);
        assert_eq!(
            lp.shortfall_for(&StopReason::UpstreamExhausted),
            Some(FetchShortfall::Exhausted)
        );
        assert_eq!(
            lp.shortfall_for(&StopReason::CursorLoop),
            Some(FetchShortfall::Exhausted)
        );
        assert_eq!(
            lp.shortfall_for(&StopReason::EmptyPageLimit),
            Some(FetchShortfall::Exhausted)
        );
    }

    /// 单测 7(DR-03 钉子):max=50,3 连页内容与第 1 页相同(cursor 各异)
    /// → 第 3 重复页 Stop(EmptyPageLimit)(empty_streak 按 newly_accepted.is_empty() 计)。
    #[test]
    fn repeated_content_pages_stop_at_empty_limit() {
        let mut lp = PaginationLoop::new(50);
        let first = ids("a", 10);
        let p1 = lp.accept_page(&first, Some("c1".into()));
        assert_eq!(p1.newly_accepted.len(), 10);
        let r1 = lp.accept_page(&first, Some("c2".into()));
        assert!(r1.newly_accepted.is_empty());
        assert!(matches!(r1.decision, PageDecision::Continue { .. }));
        let r2 = lp.accept_page(&first, Some("c3".into()));
        assert!(matches!(r2.decision, PageDecision::Continue { .. }));
        let r3 = lp.accept_page(&first, Some("c4".into()));
        assert_eq!(r3.decision, PageDecision::Stop(StopReason::EmptyPageLimit));
        assert_eq!(lp.accepted_count(), 10);
    }

    /// 单测 8(SM#F8 钉子):max=20,1 页 20 条唯一 id + next_cursor=None
    /// (末页恰好达量)→ ReachedMaxCount 优先于 UpstreamExhausted。
    #[test]
    fn combined_signal_prefers_reached_max() {
        let mut lp = PaginationLoop::new(20);
        let p = lp.accept_page(&ids("a", 20), None);
        assert_eq!(p.newly_accepted.len(), 20);
        assert_eq!(p.decision, PageDecision::Stop(StopReason::ReachedMaxCount));
        assert_eq!(lp.shortfall_for(&StopReason::ReachedMaxCount), None);
        assert_eq!(lp.accepted_count(), 20);
    }

    // ──────────────────────────────────────────────────────────────────
    // proptest 套件 PT-1~PT-4(T-030~T-033;生成器范围 AG-020~AG-023,不得收窄)
    // ──────────────────────────────────────────────────────────────────

    /// harness 驱动结果
    struct DriveResult {
        /// accept_page 实际调用次数
        steps: usize,
        /// 产生的 Stop 原因(None = 补页后仍未 Stop,违反活性)
        stop: Option<StopReason>,
        /// Stop 前(含触发 Stop 的页)实际被喂入的各页 id;Stop 后页不喂入
        fed_pages: Vec<Vec<String>>,
    }

    /// **harness 规范(Step 07 冻结)**:驱动器逐页喂入注入序列,产生 Stop 即停;
    /// **注入页耗尽 = 上游枯竭,驱动器自动补一页 `(空, None)`**。
    fn drive(
        lp: &mut PaginationLoop,
        pages: &[(Vec<String>, Option<String>)],
    ) -> DriveResult {
        let mut steps = 0usize;
        let mut fed_pages: Vec<Vec<String>> = Vec::new();
        for (item_ids, next_cursor) in pages {
            steps += 1;
            fed_pages.push(item_ids.clone());
            let out = lp.accept_page(item_ids, next_cursor.clone());
            if let PageDecision::Stop(reason) = out.decision {
                return DriveResult {
                    steps,
                    stop: Some(reason),
                    fed_pages,
                };
            }
        }
        // 注入页耗尽:自动补一页 (空, None)
        steps += 1;
        fed_pages.push(Vec::new());
        let out = lp.accept_page(&[], None);
        let stop = match out.decision {
            PageDecision::Stop(reason) => Some(reason),
            PageDecision::Continue { .. } => None,
        };
        DriveResult {
            steps,
            stop,
            fed_pages,
        }
    }

    /// 生成器(AG-020~AG-023):id 小空间 `[a-d][0-9]{0,2}`,cursor 小字母表
    /// `[a-c]{1,2}`(高概率制造环),每页条数 0..=25。
    fn page_strategy() -> impl Strategy<Value = (Vec<String>, Option<String>)> {
        (
            prop::collection::vec("[a-d][0-9]{0,2}", 0..=25),
            prop::option::of("[a-c]{1,2}"),
        )
    }

    /// 页数 0..=20
    fn pages_strategy() -> impl Strategy<Value = Vec<(Vec<String>, Option<String>)>> {
        prop::collection::vec(page_strategy(), 0..=20)
    }

    /// PT-4 用达量序列:唯一 id ≥ max_count 且 cursor 链足够(各页 cursor 互异、非空页)。
    fn saturating_pages(
        max_count: usize,
        page_size: usize,
    ) -> Vec<(Vec<String>, Option<String>)> {
        let n_pages = (max_count + page_size - 1) / page_size;
        (0..n_pages)
            .map(|p| {
                let page_ids = (0..page_size)
                    .map(|i| format!("u{}", p * page_size + i))
                    .collect();
                (page_ids, Some(format!("s{p}")))
            })
            .collect()
    }

    proptest! {
        /// PT-1(T-030 / I-001):任意序列 accepted_count ≤ max_count。
        #[test]
        fn pt1_accepted_count_never_exceeds_max(
            max_count in 1usize..=200,
            pages in pages_strategy(),
        ) {
            let mut lp = PaginationLoop::new(max_count);
            drive(&mut lp, &pages);
            prop_assert!(lp.accepted_count() <= max_count);
        }

        /// PT-2(T-031 / I-002,Step 07 DR-02 可满足形式):
        /// ① accept_page 调用次数 ≤ 序列长度 + 1;
        /// ② 必在 序列长度 + 1 步内产生 Stop(_);
        /// ③ StopReason ∈ 四枚举(完备性断言)。
        #[test]
        fn pt2_terminates_within_len_plus_one_steps(
            max_count in 1usize..=200,
            pages in pages_strategy(),
        ) {
            let mut lp = PaginationLoop::new(max_count);
            let res = drive(&mut lp, &pages);
            prop_assert!(res.steps <= pages.len() + 1);
            prop_assert!(res.stop.is_some());
            let stop = res.stop.unwrap();
            prop_assert!(matches!(
                stop,
                StopReason::ReachedMaxCount
                    | StopReason::UpstreamExhausted
                    | StopReason::CursorLoop
                    | StopReason::EmptyPageLimit
            ));
        }

        /// PT-3(T-032 / I-003):accepted_count == min(Stop 前实际被喂入页中的
        /// 唯一 id 数, max_count);Stop 后页不喂入、不计入唯一 id 基数。
        #[test]
        fn pt3_accepted_equals_min_unique_fed_ids_and_max(
            max_count in 1usize..=200,
            pages in pages_strategy(),
        ) {
            let mut lp = PaginationLoop::new(max_count);
            let res = drive(&mut lp, &pages);
            let unique_fed: HashSet<&String> = res.fed_pages.iter().flatten().collect();
            prop_assert_eq!(lp.accepted_count(), unique_fed.len().min(max_count));
        }

        /// PT-4(T-033 / I-004,双向 iff):
        /// `shortfall_for(stop)==Some(Exhausted)` ⟺ stop ∈ {UpstreamExhausted,
        /// CursorLoop, EmptyPageLimit} 且 accepted < max_count;
        /// 达量序列(唯一 id ≥ max_count 且 cursor 链足够)永不产出 Exhausted。
        #[test]
        fn pt4_shortfall_exhausted_iff_exhaustion_family_and_under_max(
            max_count in 1usize..=200,
            pages in pages_strategy(),
            page_size in 1usize..=25,
        ) {
            // (a) 任意序列上的双向 iff
            let mut lp = PaginationLoop::new(max_count);
            let res = drive(&mut lp, &pages);
            if let Some(stop) = &res.stop {
                let in_exhaustion_family = matches!(
                    stop,
                    StopReason::UpstreamExhausted
                        | StopReason::CursorLoop
                        | StopReason::EmptyPageLimit
                );
                let expected = in_exhaustion_family && lp.accepted_count() < max_count;
                prop_assert_eq!(
                    lp.shortfall_for(stop) == Some(FetchShortfall::Exhausted),
                    expected
                );
            }

            // (b) 达量序列永不产出 Exhausted
            let sat = saturating_pages(max_count, page_size);
            let mut lp2 = PaginationLoop::new(max_count);
            let res2 = drive(&mut lp2, &sat);
            prop_assert!(res2.stop.is_some());
            let stop2 = res2.stop.unwrap();
            prop_assert!(lp2.shortfall_for(&stop2) != Some(FetchShortfall::Exhausted));
        }
    }
}
