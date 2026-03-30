# Bot Identity

Snif runs in your CI pipeline and posts review comments using the credentials
you provide. The identity that appears on comments depends on the token used.


# GitHub

## Default (no setup needed)

With `GITHUB_TOKEN` (provided automatically by GitHub Actions), Snif posts
comments as "github-actions". This works out of the box — add `SNIF_API_KEY`
as the only secret and the review workflow runs.

## Custom identity with a GitHub App

To give Snif its own name and avatar on your repositories, create a GitHub
App for your organization:

1. Go to github.com/settings/apps and create a new app
2. Set a name and upload an avatar
3. Configure permissions: Pull requests (Read & Write), Contents (Read),
   Issues (Write)
4. Generate a private key and install the app on your repositories
5. Add the credentials as repository secrets:
   - `SNIF_APP_ID` — the numeric app ID
   - `SNIF_APP_PRIVATE_KEY` — the full contents of the .pem file
   - `SNIF_APP_INSTALLATION_ID` — the numeric installation ID

When these secrets are present, Snif authenticates as the app automatically.
When not present, it falls back to `GITHUB_TOKEN`.

Each organization or user creates their own GitHub App. The app credentials
are specific to the installation — they cannot be shared across organizations.


# GitLab

GitLab does not have an app installation model. Comments are posted as the
user who owns the access token. To give Snif its own identity on GitLab:

1. Create a dedicated GitLab user account (e.g., "snif-bot")
2. Set a display name and upload an avatar for the account
3. Give the account Developer access to the projects Snif will review
4. Generate a personal access token for the account with `api` scope
5. Use that token as `GITLAB_TOKEN` in the project's CI/CD variables

All review comments and discussion threads will show as posted by the bot
account.

On self-hosted enterprise instances, the GitLab admin can create a service
account specifically for this purpose. The adapter works with any valid token
regardless of how the account authenticates (local, LDAP, SAML, OAuth).
