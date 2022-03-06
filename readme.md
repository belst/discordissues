# Create Issues from Discord Messages

This bot allows you to create Github issues from Discord messages by just reacting to them with the 🐛 (:bug:) emoji

It also creates a thread and every message in the thread gets posted as a comment to the issue.

The other direction is planned but not yet implemented

## Required ENV variables

- `DISCORD_TOKEN`: Your discord bot token
- `DATABASE_URL`: Database url (only sqlite tested yet)
- `GITHUB_TOKEN`: Bearer Token for your github bot account (machine user), needs `repo` scope
- `GITHUB_REPO`: path to github repo where the issues should be created in the form `owner/repo`


## TODOS:

- Create mapping from `guild_id` to `repo`
- Check ways to get new comments from github issues (webhooks? polling?)
- What happens if you close/delete the issue/thread? Archive the thread when the issue gets closed?
