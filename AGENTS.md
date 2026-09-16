<!-- BEGIN:workhorse 0.4.0 -->
# Workhorse framework

This workspace uses [Workhorse](https://github.com/beyondessential/workhorse), a spec-driven development workbench. Workhorse ships skills (invokable prompts) and reference docs into this repo to shape how AI agents work here.

- **Skills** live at `.agents/skills/` — each skill is a folder containing a `SKILL.md` with YAML frontmatter and a prompt body. `.claude/skills/` is a symlink to the same folder so Claude Code picks them up natively
- **Reference docs** live at `.agents/docs/` — long-form guidance that skill bodies cite by path (spec format conventions and similar)
- **Specs** live at `.workhorse/specs/` — acceptance criteria for each piece of work

When picking up a task, read the skill whose folder name matches what you're being asked to do — its `SKILL.md` describes how to approach the work and which reference docs to follow.

Workhorse keeps this section, the skills, and the reference docs current automatically: the first agent turn of a session smart-merges the latest release over your local edits, so your deliberate changes survive. Edit or remove it freely.
<!-- END:workhorse -->

# House rules

## Version control

- Use jj/jujutsu locally when available and enabled for the repo.
- Use conventional commit messages. Add a Co-authored-by: line or similar.
- Always run `cargo fmt` before committing changes, even if that touches "irrelevant" files.

## Changing code

- When adding or changing features, or when fixing bugs, add tests whenever possible.
- When removing code that has already been committed, delete it unless explicitly requested that it be commented out.
- Try to keep source files under 1000 lines, splitting thematically into mods proactively.

## Rust style

- Use the newer `foo.rs` / `foo/sub.rs` style of modules.
- `use` statements always go before `mod` statements.
- Imports: merge them and group them by std, then third-party/workspace, then local (crate, super, self).
- To silence a warning, use `#[expect(..., reason = "...")]` instead of `#[allow(...)]`.

## Comments and prose

- Never write useless comments that only repeat the code. Keep comments as terse as possible.
- Don't use emojis unless absolutely necessary.

## Dependencies

- Prefer using small dependencies instead of reimplementing the wheel. Ask the user to pick a dependency if there is no obvious choice.
- When writing parsers, unless very trivial, implement them using a library like winnow or chumsky.

## This repo

- The browser client is built with `crates/bliti-web/build.sh`, which needs the `wasm32-unknown-unknown` target and `wasm-bindgen`.
- When writing or changing specs in `.workhorse/specs/` or plans in `.workhorse/plans/`, follow the spec and plan rules in [.workhorse/rules.md](.workhorse/rules.md).
