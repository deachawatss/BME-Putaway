---
date: 2026-02-16
tags: [docker, csp, angular, rust, ldap, security]
---

# CSP and Docker Configuration for Internal Network Apps

## The Problem

When containerizing an Angular + Rust application:
1. CSP blocking API calls - connect-src localhost doesn't work when accessing via IP
2. LDAP authentication failing - LDAP_SKIP_VERIFY restricted to development mode
3. Docker build failures - Lockfiles excluded, Rust version mismatches

## Solutions

### Dynamic CSP Generation

Extract host from API_BASE_URL in Dockerfile:

```dockerfile
RUN API_HOST=$(echo "${API_BASE_URL}" | sed -n 's|http://\([^:]*\):.*|\1|p') && \
    sed -i 's|<meta http-equiv="Content-Security-Policy" content="[^"]*">|<meta ... content="... http://${API_HOST}:4400 ...">|' src/index.html
```

### Internal Network Security

For internal corporate networks, remove artificial restrictions:

```rust
// Allow with warning for internal networks
tracing::warn!("LDAP SSL certificate verification is DISABLED - internal network only!");
```

### Docker Best Practices

1. Include lockfiles (don't exclude from .dockerignore)
2. Match Rust versions to lockfile format
3. Disable inlineCritical CSS if Beasties fails

## Key Pattern

Derive CSP from actual deployment URL, not localhost.
