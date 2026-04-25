# Privacy Policy

Last updated: April 25, 2026

## Overview

`dbxcli` is a command-line tool for interacting with Dropbox. This policy explains what information the tool uses and how it is handled.

## Information dbxcli accesses

When you authorize `dbxcli` with Dropbox, the tool may access Dropbox account and file information allowed by the scopes you grant. Depending on enabled permissions, this may include:

- Basic Dropbox account information, such as account ID and email address
- File and folder metadata, such as names, IDs, paths, and revision information
- File contents, when you explicitly run commands that read, upload, download, or modify files

`dbxcli` only accesses Dropbox data needed to execute the command you run.

## Local storage

`dbxcli` may store authentication credentials locally on your machine, such as access tokens or refresh tokens, if you use an authentication flow that requires persistent login. These credentials are used only to authenticate future Dropbox API requests from your local environment.

You can revoke access at any time from your Dropbox connected apps settings.

## Data collection

`dbxcli` does not operate a hosted service and does not send your Dropbox data to the project maintainers.

The tool communicates directly between your machine and Dropbox APIs. Dropbox may process API requests according to Dropbox's own privacy policy and developer terms.

## Logs and diagnostics

`dbxcli` may print command output, errors, or diagnostic information to your terminal. Avoid sharing logs publicly if they may contain file names, paths, account information, access tokens, or other sensitive data.

`dbxcli` should not intentionally print access tokens. If you discover token leakage, report it as a security issue.

## Third-party services

`dbxcli` uses Dropbox APIs to perform requested operations. No other third-party service is required for normal CLI operation unless explicitly documented by a future feature.

## Data retention

Because `dbxcli` is local software, the project maintainers do not retain your Dropbox data. Any local files, downloaded content, command output, logs, or stored credentials remain on your machine until you delete them.

## Security

Keep your Dropbox tokens and credential files secret. Anyone with access to them may be able to access your Dropbox data according to the permissions granted.

If credentials are compromised, revoke the app authorization in Dropbox and generate new credentials.

## Changes

This policy may be updated as `dbxcli` gains new features. Material changes should be reflected in this file.

## Contact

For privacy or security questions, open an issue in the project repository or contact the project maintainer.
