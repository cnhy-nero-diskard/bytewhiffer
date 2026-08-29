# OpenSpec Autoship Skill

Autonomous, hands-free execution of an OpenSpec change: applies every task
(`openspec-apply-change` loop) and commits **and pushes** each coherent
checkpoint (`auto-commit-agent` safety gate) until the whole change is on the
remote — with zero manual oversight.

## Files Created

1. `.cline/skills/openspec-autoship/SKILL.md`
   - Combined skill definition (apply loop + safety gate + autonomous push)

## How It Combines the Two Skills

| Source skill | What it contributes |
| --- | --- |
| `openspec-apply-change` | Change selection, `status`/`instructions` mechanics, context files, task loop, checkbox updates, apply guardrails |
| `auto-commit-agent` | Preflight safety gate, explicit-path staging, conventional commit messages, diff verification |
| New in this skill | Autonomy contract: no permission prompts, direct push to the current branch (including `main`), push-after-every-checkpoint, push-rejection auto-merge handling |

## Usage

```
$openspec-autoship                # auto-select the active change
$openspec-autoship add-auth       # ship a named change
$openspec-autoship --dry-run      # full pass, no commit/push
$openspec-autoship --no-push      # commit checkpoints locally only
```

## Autonomy Contract

- Commits and pushes happen automatically at every coherent checkpoint.
- Pushes go to whatever non-detached branch is checked out, including `main`.
- The only pauses are hard blockers: ambiguous change selection, unclear tasks /
  design issues, secrets in a diff, inseparable pre-existing edits, unresolvable
  merge conflicts, or a user interrupt.

## Safety Guarantees Retained

- Explicit-path staging only (never `git add -A` / `git add .`)
- Secret scan before every commit
- `git diff --cached --check` before every commit
- No force-push, no rebase, no amend, no history rewrite
- Pre-existing unrelated dirty files are never staged, committed, or pushed
- Quick project checks (build/test/lint) before each checkpoint

## Notes

- Archiving the change is intentionally left to the user
  (`$openspec-archive-change`) — only implementation and shipping are automated.
- If `git push` is rejected because the remote moved ahead, the skill merges
  (`git pull --no-rebase`) and continues; it stops only on real conflicts.
