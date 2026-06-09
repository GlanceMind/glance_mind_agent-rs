# Requirements Ledger

Status legend: CONFIRMED / ASSUMED. At handoff none may be ASSUMED.

| ID | Requirement | Source | Status |
|----|-------------|--------|--------|
| R1 | A TikTok keyword/hashtag search task with `max_scan_count=N` fetches up to N items (not capped at 20), bounded by real supply & `has_more`. | User bug report + debug | CONFIRMED |
| R2 | A TikTok user/page task (`UserId`/`SecUserId`) with target N likewise paginates up to N. | Debug (same root cause on user path) | CONFIRMED |
| R3 | Per-request page size stays ≤20 (TikHub hard cap). | API constraint, code comments | CONFIRMED |
| R4 | Pagination terminates on target reached / empty page / `has_more!=1` / missing cursor / max-page guard. | First principles C4 | CONFIRMED |
| R5 | No duplicate `aweme_id` is returned across pages. | First principles C5 | CONFIRMED |
| R6 | "Empty search result → end campaign" still fires only for a truly empty FIRST page. | orchestrator.rs:470 | CONFIRMED |
| R7 | Quota: fetching N≈100 may issue ~5 requests; acceptable, fetch to config. | User decision | CONFIRMED |
| R8 | Tests use a mock HTTP server (mockito) exercising the real client request/response/loop path. | User decision | CONFIRMED |
| R9 | Scope limited to TikTok/TikHub; other platforms untouched. | User decision | CONFIRMED |
| R10 | `ContentId` (single post) path unchanged. | First principles | CONFIRMED |
