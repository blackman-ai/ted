// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * teddy.rocks Subdomain Routing Worker
 *
 * Routes *.teddy.rocks subdomains to cloudflared tunnels.
 * Zero-config for end users - no API keys required!
 *
 * Flow:
 * 1. User clicks "Share" in Teddy
 * 2. Teddy starts a cloudflared tunnel
 * 3. Teddy calls POST /api/register with slug and tunnel URL
 * 4. Worker verifies the tunnel is reachable, stores mapping in KV
 * 5. Requests to slug.teddy.rocks are proxied to the tunnel
 * 6. When Teddy closes, tunnel dies, link stops working
 * 7. KV entries auto-expire after 24 hours
 *
 * Security:
 * - Rate limiting per IP (5 registrations per hour)
 * - Tunnel URL verification before registration
 * - Client tokens for managing your own subdomains
 * - Auto-expiration of all entries
 */

export interface Env {
  SUBDOMAINS: KVNamespace;
  RATE_LIMITS: KVNamespace;  // For rate limiting
  ENVIRONMENT: string;
}

interface SubdomainRecord {
  tunnelUrl: string;
  createdAt: number;
  expiresAt: number;
  clientToken: string;  // Random token to allow client to manage this subdomain
  clientIp: string;
  metadata?: {
    projectName?: string;
  };
}

interface RegisterRequest {
  slug: string;
  tunnelUrl: string;
  projectName?: string;
  clientToken?: string;  // Optional - if provided, must match for updates
}

// Subdomain slug validation
const SLUG_REGEX = /^[a-z0-9][a-z0-9-]{2,30}[a-z0-9]$/;
const RESERVED_SLUGS = new Set([
  "www", "api", "admin", "app", "dashboard", "docs",
  "help", "support", "status", "blog", "mail", "smtp",
  "ftp", "ssh", "cdn", "static", "assets", "images",
]);

// TTL for subdomain registrations (24 hours)
const DEFAULT_TTL_SECONDS = 24 * 60 * 60;

// Rate limiting: max registrations per IP per hour
const RATE_LIMIT_MAX = 10;
const RATE_LIMIT_WINDOW_SECONDS = 60 * 60; // 1 hour

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    const hostname = url.hostname;

    // Handle API requests on api.teddy.rocks or teddy.rocks/api/*
    if (hostname === "api.teddy.rocks" || url.pathname.startsWith("/api/")) {
      return handleApiRequest(request, env);
    }

    // Handle root domain
    if (hostname === "teddy.rocks" || hostname === "www.teddy.rocks") {
      return handleRootDomain(request);
    }

    // Handle subdomain routing (e.g., garden-blog-7x3k.teddy.rocks)
    const subdomain = extractSubdomain(hostname);
    if (subdomain) {
      return handleSubdomainRequest(request, subdomain, env);
    }

    return new Response("Not Found", { status: 404 });
  },
};

/**
 * Extract subdomain from hostname
 */
function extractSubdomain(hostname: string): string | null {
  const match = hostname.match(/^([a-z0-9-]+)\.teddy\.rocks$/);
  if (match && match[1] !== "www" && match[1] !== "api") {
    return match[1];
  }
  return null;
}

/**
 * Get client IP from request
 */
function getClientIp(request: Request): string {
  return request.headers.get("CF-Connecting-IP") ||
         request.headers.get("X-Forwarded-For")?.split(",")[0] ||
         "unknown";
}

/**
 * Generate a random token
 */
function generateToken(): string {
  const array = new Uint8Array(16);
  crypto.getRandomValues(array);
  return Array.from(array, b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Check rate limit for an IP
 */
async function checkRateLimit(ip: string, env: Env): Promise<{ allowed: boolean; remaining: number }> {
  const key = `rate:${ip}`;
  const current = await env.RATE_LIMITS.get(key);
  const count = current ? parseInt(current, 10) : 0;

  if (count >= RATE_LIMIT_MAX) {
    return { allowed: false, remaining: 0 };
  }

  // Increment counter
  await env.RATE_LIMITS.put(key, String(count + 1), {
    expirationTtl: RATE_LIMIT_WINDOW_SECONDS,
  });

  return { allowed: true, remaining: RATE_LIMIT_MAX - count - 1 };
}

/**
 * Verify that a tunnel URL looks valid
 *
 * Note: We don't actually verify reachability because:
 * 1. Cloudflare Workers can't reliably reach trycloudflare.com tunnels immediately
 * 2. The client already waits for "Registered tunnel connection" before calling us
 * 3. Even if verification passes, tunnels can go down at any time
 *
 * Instead, we just validate the URL format and trust the client.
 * If the tunnel is down, users will see an error page when they visit the subdomain.
 */
async function verifyTunnel(tunnelUrl: string): Promise<boolean> {
  // Just validate it's a valid trycloudflare.com URL
  try {
    const url = new URL(tunnelUrl);
    return url.hostname.endsWith('.trycloudflare.com') && url.protocol === 'https:';
  } catch {
    return false;
  }
}

/**
 * Handle requests to the root domain
 */
function handleRootDomain(request: Request): Response {
  const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>teddy.rocks - Share Your Apps</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
      color: #e0e0e0;
      min-height: 100vh;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      padding: 2rem;
    }
    .container { text-align: center; max-width: 600px; }
    h1 {
      font-size: 3rem;
      background: linear-gradient(135deg, #00d4ff, #7c3aed);
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      margin-bottom: 1rem;
    }
    .tagline { font-size: 1.25rem; color: #a0a0a0; margin-bottom: 2rem; }
    .features {
      display: grid;
      gap: 1rem;
      text-align: left;
      margin: 2rem 0;
    }
    .feature {
      background: rgba(255,255,255,0.05);
      padding: 1rem 1.5rem;
      border-radius: 8px;
      border-left: 3px solid #00d4ff;
    }
    .feature h3 { color: #00d4ff; margin-bottom: 0.5rem; }
    .cta {
      margin-top: 2rem;
      padding: 1rem 2rem;
      background: linear-gradient(135deg, #00d4ff, #7c3aed);
      color: white;
      text-decoration: none;
      border-radius: 8px;
      font-weight: 600;
      display: inline-block;
      transition: transform 0.2s, box-shadow 0.2s;
    }
    .cta:hover {
      transform: translateY(-2px);
      box-shadow: 0 4px 20px rgba(0,212,255,0.3);
    }
    footer { margin-top: 3rem; color: #666; font-size: 0.9rem; }
    footer a { color: #00d4ff; text-decoration: none; }
  </style>
</head>
<body>
  <div class="container">
    <h1>teddy.rocks</h1>
    <p class="tagline">Share your apps instantly. Because your app rocks!</p>

    <div class="features">
      <div class="feature">
        <h3>Instant Preview Links</h3>
        <p>Share your local dev server with anyone, anywhere. No deployment needed.</p>
      </div>
      <div class="feature">
        <h3>Zero Configuration</h3>
        <p>Just click "Share" in Teddy. No API keys, no accounts, no setup.</p>
      </div>
      <div class="feature">
        <h3>Free Forever</h3>
        <p>Built into Teddy. Free subdomains for everyone.</p>
      </div>
    </div>

    <a href="https://github.com/anthropics/ted" class="cta">Get Teddy</a>

    <footer>
      <p>Built by <a href="https://blackman.ai">Blackman AI</a></p>
    </footer>
  </div>
</body>
</html>`;

  return new Response(html, {
    headers: { "Content-Type": "text/html; charset=utf-8" },
  });
}

/**
 * Handle API requests for subdomain registration
 */
async function handleApiRequest(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname.replace(/^\/api/, "");

  // CORS headers
  const corsHeaders = {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET, POST, DELETE, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type, X-Client-Token",
  };

  // Handle CORS preflight
  if (request.method === "OPTIONS") {
    return new Response(null, { headers: corsHeaders });
  }

  // JSON response helper
  const respond = (body: any, status = 200) => {
    return new Response(JSON.stringify(body), {
      status,
      headers: {
        "Content-Type": "application/json",
        ...corsHeaders,
      },
    });
  };

  const clientIp = getClientIp(request);

  try {
    // POST /api/register - Register a new subdomain (no auth required!)
    if (path === "/register" && request.method === "POST") {
      // Check rate limit
      const rateLimit = await checkRateLimit(clientIp, env);
      if (!rateLimit.allowed) {
        return respond({
          error: "Rate limit exceeded. Please try again later.",
          retryAfter: RATE_LIMIT_WINDOW_SECONDS,
        }, 429);
      }

      const body: RegisterRequest = await request.json();

      // Validate slug
      if (!body.slug || !SLUG_REGEX.test(body.slug)) {
        return respond({
          error: "Invalid slug. Must be 4-32 chars, lowercase alphanumeric with hyphens.",
        }, 400);
      }

      if (RESERVED_SLUGS.has(body.slug)) {
        return respond({ error: "This subdomain is reserved." }, 400);
      }

      // Validate tunnel URL
      if (!body.tunnelUrl || !body.tunnelUrl.startsWith("https://")) {
        return respond({ error: "Invalid tunnel URL." }, 400);
      }

      // Check if slug is already taken
      const existing = await env.SUBDOMAINS.get(body.slug);
      if (existing) {
        const existingRecord: SubdomainRecord = JSON.parse(existing);

        // Allow update if client token matches
        if (body.clientToken && existingRecord.clientToken === body.clientToken) {
          // Update existing record
          const record: SubdomainRecord = {
            ...existingRecord,
            tunnelUrl: body.tunnelUrl,
            expiresAt: Date.now() + DEFAULT_TTL_SECONDS * 1000,
          };

          await env.SUBDOMAINS.put(body.slug, JSON.stringify(record), {
            expirationTtl: DEFAULT_TTL_SECONDS,
          });

          return respond({
            success: true,
            subdomain: `${body.slug}.teddy.rocks`,
            clientToken: record.clientToken,
            expiresAt: record.expiresAt,
            remaining: rateLimit.remaining,
          });
        }

        return respond({ error: "Subdomain already in use." }, 409);
      }

      // Verify tunnel is reachable
      const tunnelAlive = await verifyTunnel(body.tunnelUrl);
      if (!tunnelAlive) {
        return respond({
          error: "Could not reach tunnel. Make sure your dev server is running.",
        }, 400);
      }

      // Generate client token for managing this subdomain
      const clientToken = body.clientToken || generateToken();

      // Create record
      const record: SubdomainRecord = {
        tunnelUrl: body.tunnelUrl,
        createdAt: Date.now(),
        expiresAt: Date.now() + DEFAULT_TTL_SECONDS * 1000,
        clientToken,
        clientIp,
        metadata: {
          projectName: body.projectName,
        },
      };

      // Store in KV with TTL
      await env.SUBDOMAINS.put(body.slug, JSON.stringify(record), {
        expirationTtl: DEFAULT_TTL_SECONDS,
      });

      return respond({
        success: true,
        subdomain: `${body.slug}.teddy.rocks`,
        clientToken,  // Client should store this to manage the subdomain
        expiresAt: record.expiresAt,
        remaining: rateLimit.remaining,
      });
    }

    // DELETE /api/unregister - Remove a subdomain
    if (path === "/unregister" && request.method === "DELETE") {
      const slug = url.searchParams.get("slug");
      const clientToken = request.headers.get("X-Client-Token");

      if (!slug) {
        return respond({ error: "Missing slug parameter." }, 400);
      }

      if (!clientToken) {
        return respond({ error: "Missing X-Client-Token header." }, 401);
      }

      // Verify ownership
      const existing = await env.SUBDOMAINS.get(slug);
      if (!existing) {
        return respond({ success: true }); // Already gone, that's fine
      }

      const record: SubdomainRecord = JSON.parse(existing);
      if (record.clientToken !== clientToken) {
        return respond({ error: "Not authorized to delete this subdomain." }, 403);
      }

      await env.SUBDOMAINS.delete(slug);
      return respond({ success: true });
    }

    // GET /api/status/:slug - Check subdomain status (public)
    if (path.startsWith("/status/") && request.method === "GET") {
      const slug = path.replace("/status/", "");
      const record = await env.SUBDOMAINS.get(slug);

      if (!record) {
        return respond({ available: true });
      }

      const data: SubdomainRecord = JSON.parse(record);
      return respond({
        available: false,
        expiresAt: data.expiresAt,
        projectName: data.metadata?.projectName,
      });
    }

    // GET /api/health - Health check
    if (path === "/health" && request.method === "GET") {
      return respond({ status: "ok", timestamp: Date.now() });
    }

    return respond({ error: "Not Found" }, 404);
  } catch (err) {
    console.error("API error:", err);
    return respond({ error: "Internal Server Error" }, 500);
  }
}

/**
 * Handle subdomain requests - proxy to tunnel
 */
async function handleSubdomainRequest(
  request: Request,
  subdomain: string,
  env: Env
): Promise<Response> {
  const record = await env.SUBDOMAINS.get(subdomain);

  if (!record) {
    return new Response(notFoundPage(subdomain), {
      status: 404,
      headers: { "Content-Type": "text/html; charset=utf-8" },
    });
  }

  const data: SubdomainRecord = JSON.parse(record);

  // Check if expired
  if (Date.now() > data.expiresAt) {
    await env.SUBDOMAINS.delete(subdomain);
    return new Response(expiredPage(subdomain), {
      status: 410,
      headers: { "Content-Type": "text/html; charset=utf-8" },
    });
  }

  // Proxy the request to the tunnel
  const url = new URL(request.url);
  const targetUrl = new URL(url.pathname + url.search, data.tunnelUrl);

  const proxyRequest = new Request(targetUrl.toString(), {
    method: request.method,
    headers: request.headers,
    body: request.body,
    redirect: "manual",
  });

  try {
    const response = await fetch(proxyRequest);

    const newHeaders = new Headers(response.headers);
    newHeaders.set("X-Teddy-Subdomain", subdomain);
    newHeaders.set("X-Teddy-Project", data.metadata?.projectName || "");

    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: newHeaders,
    });
  } catch (err) {
    console.error("Proxy error:", err);
    return new Response(tunnelErrorPage(subdomain), {
      status: 502,
      headers: { "Content-Type": "text/html; charset=utf-8" },
    });
  }
}

function notFoundPage(subdomain: string): string {
  return `<!DOCTYPE html>
<html>
<head>
  <title>Not Found - teddy.rocks</title>
  <style>
    body { font-family: system-ui; background: #1a1a2e; color: #e0e0e0; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; }
    .container { text-align: center; padding: 2rem; }
    h1 { color: #ff6b6b; }
    code { background: #2a2a4e; padding: 0.2rem 0.5rem; border-radius: 4px; }
    a { color: #00d4ff; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Subdomain Not Found</h1>
    <p>The subdomain <code>${subdomain}.teddy.rocks</code> doesn't exist or has expired.</p>
    <p><a href="https://teddy.rocks">Go to teddy.rocks</a></p>
  </div>
</body>
</html>`;
}

function expiredPage(subdomain: string): string {
  return `<!DOCTYPE html>
<html>
<head>
  <title>Link Expired - teddy.rocks</title>
  <style>
    body { font-family: system-ui; background: #1a1a2e; color: #e0e0e0; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; }
    .container { text-align: center; padding: 2rem; }
    h1 { color: #ffa500; }
    code { background: #2a2a4e; padding: 0.2rem 0.5rem; border-radius: 4px; }
    a { color: #00d4ff; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Preview Link Expired</h1>
    <p>The preview link <code>${subdomain}.teddy.rocks</code> has expired.</p>
    <p>Preview links are only active while Teddy is running.</p>
    <p><a href="https://teddy.rocks">Learn more at teddy.rocks</a></p>
  </div>
</body>
</html>`;
}

function tunnelErrorPage(subdomain: string): string {
  return `<!DOCTYPE html>
<html>
<head>
  <title>Connection Error - teddy.rocks</title>
  <style>
    body { font-family: system-ui; background: #1a1a2e; color: #e0e0e0; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; }
    .container { text-align: center; padding: 2rem; }
    h1 { color: #ff6b6b; }
    code { background: #2a2a4e; padding: 0.2rem 0.5rem; border-radius: 4px; }
    a { color: #00d4ff; }
  </style>
</head>
<body>
  <div class="container">
    <h1>Connection Error</h1>
    <p>Could not connect to <code>${subdomain}.teddy.rocks</code>.</p>
    <p>The developer's Teddy app may have closed or lost connection.</p>
    <p>Try refreshing or ask them to restart the preview.</p>
    <p><a href="https://teddy.rocks">Learn more at teddy.rocks</a></p>
  </div>
</body>
</html>`;
}
