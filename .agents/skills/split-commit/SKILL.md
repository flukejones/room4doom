# Split Commit Skill
Split the current HEAD commit into multiple focused, compilable commits.
1. Run `git diff HEAD~1` to see all changes
2. Group changes by logical unit (file moves, refactors, bug fixes, features)
3. For each group: `git reset HEAD~1`, stage files, commit with descriptive message
4. Verify each commit compiles with `cargo check`
