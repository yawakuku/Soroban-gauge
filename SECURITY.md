# Security policy

Report vulnerabilities privately through GitHub's "Report a vulnerability" button
(Security tab) rather than a public issue. Include the version, a description and
steps to reproduce. You can expect an acknowledgement within 7 days.

soroban-gauge reads local files and, only in the optional `calibrate` command,
makes read-only simulation requests. It never signs or submits transactions and
never reads secret keys.
