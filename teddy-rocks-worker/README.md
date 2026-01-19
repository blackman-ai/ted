# teddy.rocks Subdomain Service

Cloudflare Worker that powers the teddy.rocks subdomain routing service.

## Overview

This worker enables instant preview links for Teddy users. When a user clicks "Share" in Teddy, they get a URL like `myapp-xyz.teddy.rocks` that routes to their local development server via a Cloudflare tunnel.

## Setup

### Prerequisites

- Cloudflare account with teddy.rocks domain
- Node.js 18+
- Wrangler CLI (`npm install -g wrangler`)

### Installation

```bash
cd teddy-rocks-worker
npm install
```

### Configuration

1. **Login to Cloudflare:**
   ```bash
   wrangler login
   ```

2. **Create KV namespaces:**
   ```bash
   # Production namespace
   wrangler kv:namespace create "SUBDOMAINS"
   # Note the ID from the output

   # Preview namespace (for local dev)
   wrangler kv:namespace create "SUBDOMAINS" --preview
   # Note the preview_id from the output
   ```

3. **Update `wrangler.toml`** with the namespace IDs from step 2.

4. **Create rate limiting KV namespace:**
   ```bash
   wrangler kv:namespace create "RATE_LIMITS"
   wrangler kv:namespace create "RATE_LIMITS" --preview
   ```

5. **Update `wrangler.toml`** with the RATE_LIMITS namespace IDs

### Development

```bash
npm run dev
```

This starts a local worker at http://localhost:8787

To test locally:
```bash
curl http://localhost:8787/api/health
```

### Deployment

```bash
npm run deploy
```

After deploying, verify:
```bash
curl https://teddy.rocks/api/health
```

### DNS Configuration

Ensure these records exist in Cloudflare DNS for teddy.rocks:

| Type | Name | Content | Proxy |
|------|------|---------|-------|
| A | @ | 192.0.2.1 | Proxied |
| A | * | 192.0.2.1 | Proxied |

The A records can point to any IP since the Worker handles all requests.

## API Endpoints

**Note:** No API keys required! The service uses rate limiting and client tokens for security.

### POST /api/register

Register a new subdomain. No authentication needed.

**Body:**
```json
{
  "slug": "my-app-xyz",
  "tunnelUrl": "https://abc123.trycloudflare.com",
  "projectName": "My App",
  "clientToken": "optional-token-for-updates"
}
```

**Response:**
```json
{
  "success": true,
  "subdomain": "my-app-xyz.teddy.rocks",
  "clientToken": "abc123...",
  "expiresAt": 1704067200000,
  "remaining": 9
}
```

The `clientToken` should be stored locally to manage (update/delete) this subdomain later.

### DELETE /api/unregister?slug=my-app-xyz

Remove a subdomain registration.

**Headers:**
- `X-Client-Token: <clientToken>` (required - the token returned from registration)

### GET /api/status/:slug

Check if a subdomain is available or in use.

**Response:**
```json
{
  "available": false,
  "expiresAt": 1704067200000,
  "projectName": "My App"
}
```

### GET /api/health

Health check endpoint.

## How It Works

1. User clicks "Share" in Teddy
2. Teddy starts a cloudflared tunnel to their local dev server
3. Teddy generates a unique slug (e.g., `garden-blog-7x3k`)
4. Teddy calls `POST /api/register` with the slug and tunnel URL
5. Worker stores the mapping in Cloudflare KV
6. Requests to `garden-blog-7x3k.teddy.rocks` are proxied to the tunnel
7. When Teddy closes, it calls `DELETE /api/unregister` to clean up
8. KV entries auto-expire after 24 hours as a fallback

## Security

- **Zero-config for users** - No API keys needed to use the service
- **Rate limiting** - Max 10 registrations per IP per hour
- **Client tokens** - Randomly generated tokens allow managing your own subdomains
- **Tunnel verification** - Tunnels are verified as reachable before registration
- **Auto-expiration** - All subdomains expire after 24 hours
- **Reserved slugs** - Common subdomains (www, api, admin, etc.) are blocked
- **Slug validation** - Prevents injection attacks

## License

AGPL-3.0-or-later
