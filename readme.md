# Create Issues from Discord Messages

This bot allows you to create Github issues from Discord messages by just reacting to them with the 🐛 (:bug:) emoji

It also creates a thread and every message in the thread gets posted as a comment to the issue.

The other direction is planned but not yet implemented

## Configuration

Configuration can be passed via the `--config` command line argument (Default `config.toml`)

See [`example_config.toml`](example_config.toml) for an example

## TODOS:

- Check ways to get new comments from github issues (webhooks? polling?)
- What happens if you close/delete the issue/thread? Archive the thread when the issue gets closed?
