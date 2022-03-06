# Create Issues from Discord Messages

This bot allows you to create Github issues from Discord messages by just reacting to them with the 🐛 (`:bug:`) emoji

It also creates a thread and every message in the thread gets posted as a comment to the issue.  
Posting comments to the issue will also create a message in the thread if webhooks are configured.

## Configuration

Configuration can be passed via the `--config` command line argument (Default `config.toml`)

See [`example_config.toml`](example_config.toml) for an example.

To enable webhooks create a webhook in github to `http://your.domain.tld:8080/webhook` with only `issue_comment` events.

## TODOS:

- What happens if you close/delete the issue/thread? Archive the thread when the issue gets closed?
- more edgecases
- webhooks configurable
    - automatically create on startup if github token has permissions?
    - enable/disable via config
    - configurable socket bind for webserver
