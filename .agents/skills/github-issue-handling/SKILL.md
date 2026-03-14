---
name: github-issue-handling
description: Automate the process of resolving GitHub issues from fetching information to creating a linked Pull Request.
---

# Issue Handling Skill

This skill guides the AI assistant through the end-to-end process of handling GitHub issues in the Aura project.

## 1. Information Gathering

When a user provides a GitHub issue number or link, use the `github-mcp-server` to fetch detailed information.

- **Retrieve Issue Details**: Use `github-mcp-server_issue_read` with the `get` method to understand the problem description, labels, and current status.
- **Read Comments**: Use `github-mcp-server_issue_read` with the `get_comments` method to gather additional context or hints provided in the discussion.

## 2. Environment Setup

Once the issue is understood, prepare the local environment for implementation.

- **Sync Repository**: Ensure the local repository is up to date with the default branch (`main` or `master`).
- **Create Feature Branch**: Create a new branch with a descriptive name related to the issue.
  - Pattern: `fix/issue-<number>-<short-description>` or `feat/issue-<number>-<short-description>`.
  - Use `git checkout -b <branch-name>` or `github-mcp-server_create_branch`.

## 3. Implementation and Verification

Navigate the codebase and implement the fix or feature.

- **Analyze Codebase**: Use `grep_search`, `find_by_name`, and `view_file` to locate relevant code sections.
- **Apply Changes**: Use `replace_file_content` or `multi_replace_file_content` to implement the solution.
- **Format Code**: Run `cargo fmt` to ensure the code adheres to the project's styling guidelines.
- **Verify with Tests**:
  - Run existing unit tests: `cargo test`.
  - Run integration tests: `cargo test --test '*'`.
  - Create new tests if necessary to prevent regression.

## 4. Pull Request Creation

After verifying the fix, push the changes and create a Pull Request that links back to the original issue.

- **Push Changes**: Push the feature branch to the remote repository.
- **Create PR**: Use `github-mcp-server_create_pull_request`.
- **Link Issue**: In the PR body, include a linking keyword followed by the issue number (e.g., `Closes #123` or `Fixes #123`). This ensures the issue is automatically closed when the PR is merged.
- **Request Review**: Optionally, use `github-mcp-server_update_pull_request` to add reviewers if required by the project workflow.
