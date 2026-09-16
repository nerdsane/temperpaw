# Decisions and tradeoffs

## Use the existing TemperPaw dashboard

**Decision:** Add Foresight to the authenticated TemperPaw dashboard.

**Came up because:** The user asked for a working interface as soon as possible, and the historical Temper-based Deep Sci-Fi frontend is in a closed, unmerged 117-file rewrite.

**Options:** Extend the current dashboard; revive the separate Next.js rewrite; build another standalone frontend.

**Chose the current dashboard because:** It already provides authenticated OData reads/actions and an event stream, keeping backend and UI in the same repository. This gives up the historical frontend's presentation work, which remains reference material.

**Where:** dashboard/src/routes/foresight; PR https://github.com/nerdsane/temperpaw/pull/526.

## Use an isolated worktree on the functioning governed computer

**Decision:** Work in a dedicated git worktree on Computers('arni-big') while its requested copy remains unresolved.

**Came up because:** Computer.Copy created 01a0a7c4-ad8d-7e20-b5d9-54b05ab62f08 in CopyUnknown, and reconciliation returned HTTP 404, while governed Exec on arni-big succeeded.

**Options:** Wait for the unresolved copy; use an isolated git worktree on arni-big; move implementation to the laptop.

**Chose the isolated worktree on arni-big because:** It retains the prescribed governed execution path and keeps other checkouts untouched while allowing progress. The shared machine requires disjoint worktrees and scoped build directories.

**Where:** /home/tl-user/worktrees/arn-518-foresight, branch codex/arn-518-learning-foresight, GitHub remote nerdsane/temperpaw.
