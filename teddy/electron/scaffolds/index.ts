/**
 * Scaffolds for Teddy - Pre-built app templates that models can use
 *
 * These scaffolds help smaller models create complete, working apps by:
 * 1. Providing the boilerplate structure
 * 2. Showing where to put custom logic
 * 3. Including best practices (CSS reset, proper HTML structure, etc.)
 */

export interface ScaffoldFile {
  path: string;
  content: string;
  description: string;  // Tells the model what this file is for
}

export interface Scaffold {
  name: string;
  description: string;
  keywords: string[];  // Used for auto-detection
  files: ScaffoldFile[];
  instructions: string;  // Extra instructions for the model
}

/**
 * Vanilla HTML/CSS/JS Web App scaffold
 */
export const vanillaWebApp: Scaffold = {
  name: 'vanilla-web-app',
  description: 'Simple web app with HTML, CSS, and JavaScript',
  keywords: ['web', 'html', 'website', 'page', 'game', 'app', 'simple', 'vanilla', 'basic'],
  files: [
    {
      path: 'index.html',
      description: 'Main HTML file - add your UI structure here',
      content: `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{APP_TITLE}}</title>
    <link rel="stylesheet" href="styles.css">
</head>
<body>
    <div id="app">
        <!-- APP_CONTENT: Add your HTML structure here -->
    </div>
    <script src="app.js"></script>
</body>
</html>`
    },
    {
      path: 'styles.css',
      description: 'Styles - includes CSS reset, add your custom styles',
      content: `/* CSS Reset */
*, *::before, *::after {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
}

html {
    font-size: 16px;
    -webkit-font-smoothing: antialiased;
}

body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
    line-height: 1.6;
    color: #333;
    background: #f5f5f5;
    min-height: 100vh;
}

#app {
    max-width: 1200px;
    margin: 0 auto;
    padding: 20px;
}

/* Utility Classes */
.hidden { display: none !important; }
.flex { display: flex; }
.flex-col { flex-direction: column; }
.items-center { align-items: center; }
.justify-center { justify-content: center; }
.gap-1 { gap: 0.5rem; }
.gap-2 { gap: 1rem; }
.gap-3 { gap: 1.5rem; }

/* Button Styles */
button, .btn {
    padding: 10px 20px;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    font-size: 1rem;
    font-weight: 500;
    transition: all 0.2s ease;
}

.btn-primary {
    background: #3b82f6;
    color: white;
}

.btn-primary:hover {
    background: #2563eb;
}

/* Input Styles */
input, textarea, select {
    padding: 10px 14px;
    border: 1px solid #ddd;
    border-radius: 6px;
    font-size: 1rem;
    width: 100%;
    transition: border-color 0.2s ease;
}

input:focus, textarea:focus, select:focus {
    outline: none;
    border-color: #3b82f6;
}

/* Card Styles */
.card {
    background: white;
    border-radius: 12px;
    padding: 20px;
    box-shadow: 0 2px 8px rgba(0,0,0,0.1);
}

/* APP_STYLES: Add your custom styles below */
`
    },
    {
      path: 'app.js',
      description: 'JavaScript - includes state management pattern, add your logic',
      content: `/**
 * {{APP_TITLE}} - Main Application
 */

// ============================================
// STATE MANAGEMENT
// ============================================

const state = {
    // APP_STATE: Define your app state here
    // Example: items: [], currentUser: null, score: 0
};

function updateState(updates) {
    Object.assign(state, updates);
    render();
    saveState();
}

// ============================================
// LOCAL STORAGE
// ============================================

const STORAGE_KEY = '{{APP_STORAGE_KEY}}';

function saveState() {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch (e) {
        console.warn('Could not save state:', e);
    }
}

function loadState() {
    try {
        const saved = localStorage.getItem(STORAGE_KEY);
        if (saved) {
            Object.assign(state, JSON.parse(saved));
        }
    } catch (e) {
        console.warn('Could not load state:', e);
    }
}

// ============================================
// DOM HELPERS
// ============================================

const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => document.querySelectorAll(selector);

function createElement(tag, attrs = {}, children = []) {
    const el = document.createElement(tag);
    Object.entries(attrs).forEach(([key, value]) => {
        if (key === 'className') el.className = value;
        else if (key === 'onClick') el.addEventListener('click', value);
        else if (key.startsWith('data')) el.setAttribute(key.replace(/([A-Z])/g, '-$1').toLowerCase(), value);
        else el[key] = value;
    });
    children.forEach(child => {
        if (typeof child === 'string') el.appendChild(document.createTextNode(child));
        else if (child) el.appendChild(child);
    });
    return el;
}

// ============================================
// RENDER FUNCTION
// ============================================

function render() {
    const app = $('#app');
    // APP_RENDER: Update DOM based on state
    // Example: app.innerHTML = \`<h1>Score: \${state.score}</h1>\`;
}

// ============================================
// EVENT HANDLERS
// ============================================

// APP_HANDLERS: Add your event handler functions here
// Example:
// function handleClick(e) {
//     updateState({ score: state.score + 1 });
// }

// ============================================
// INITIALIZATION
// ============================================

function init() {
    loadState();
    render();

    // APP_INIT: Set up event listeners
    // Example: $('#button').addEventListener('click', handleClick);
}

// Start the app when DOM is ready
document.addEventListener('DOMContentLoaded', init);
`
    }
  ],
  instructions: `
When using this scaffold:
1. Replace {{APP_TITLE}} with the actual app name
2. Replace {{APP_STORAGE_KEY}} with a unique storage key (e.g., 'tictactoe_state')
3. Fill in the sections marked with APP_STATE, APP_CONTENT, APP_STYLES, APP_RENDER, APP_HANDLERS, APP_INIT
4. Keep the existing structure - just add your code in the marked sections
5. The CSS includes utility classes you can use in your HTML
`
};

/**
 * React + Vite scaffold
 */
export const reactViteApp: Scaffold = {
  name: 'react-vite-app',
  description: 'React application with Vite bundler',
  keywords: ['react', 'component', 'jsx', 'vite', 'modern', 'spa', 'single page'],
  files: [
    {
      path: 'package.json',
      description: 'Dependencies - includes React, Vite, and common libraries',
      content: `{
  "name": "{{APP_NAME}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  },
  "devDependencies": {
    "@vitejs/plugin-react": "^4.2.0",
    "vite": "^5.0.0"
  }
}`
    },
    {
      path: 'index.html',
      description: 'HTML entry point',
      content: `<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{{APP_TITLE}}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.jsx"></script>
  </body>
</html>`
    },
    {
      path: 'vite.config.js',
      description: 'Vite configuration',
      content: `import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
})`
    },
    {
      path: 'src/main.jsx',
      description: 'React entry point',
      content: `import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './styles.css'

ReactDOM.createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)`
    },
    {
      path: 'src/App.jsx',
      description: 'Main App component - build your UI here',
      content: `import { useState, useEffect } from 'react'

// APP_IMPORTS: Import your components here

function App() {
  // APP_STATE: Define your state hooks here
  // Example: const [count, setCount] = useState(0)

  // APP_EFFECTS: Add useEffect hooks here
  // Example: useEffect(() => { loadData() }, [])

  // APP_HANDLERS: Define your event handlers here
  // Example: const handleClick = () => setCount(c => c + 1)

  return (
    <div className="app">
      {/* APP_CONTENT: Build your UI here */}
      <h1>{{APP_TITLE}}</h1>
    </div>
  )
}

export default App`
    },
    {
      path: 'src/styles.css',
      description: 'Global styles',
      content: `/* CSS Reset */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  line-height: 1.6;
  color: #333;
  background: #f5f5f5;
  min-height: 100vh;
}

.app {
  max-width: 1200px;
  margin: 0 auto;
  padding: 20px;
}

/* APP_STYLES: Add your custom styles below */
`
    }
  ],
  instructions: `
When using this scaffold:
1. Replace {{APP_NAME}} and {{APP_TITLE}} with actual values
2. Fill in sections marked with APP_STATE, APP_CONTENT, APP_HANDLERS, etc.
3. Create additional components in src/components/ if needed
4. After creating files, run: shell("npm install")
5. Tell user to run: npm run dev
`
};

/**
 * Next.js Full-Stack App scaffold
 * This is the recommended scaffold for apps needing backend functionality
 */
export const nextJsApp: Scaffold = {
  name: 'nextjs-fullstack',
  description: 'Next.js full-stack app with API routes',
  keywords: ['next', 'nextjs', 'fullstack', 'full-stack', 'react', 'api'],
  files: [
    {
      path: 'package.json',
      description: 'Dependencies - Next.js with React',
      content: `{
  "name": "{{APP_NAME}}",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start"
  },
  "dependencies": {
    "next": "14.0.4",
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  }
}`
    },
    {
      path: 'next.config.js',
      description: 'Next.js configuration',
      content: `/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
}

module.exports = nextConfig`
    },
    {
      path: 'app/layout.js',
      description: 'Root layout - wraps all pages',
      content: `import './globals.css'

export const metadata = {
  title: '{{APP_TITLE}}',
  description: '{{APP_DESCRIPTION}}',
}

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}`
    },
    {
      path: 'app/globals.css',
      description: 'Global styles',
      content: `/* CSS Reset */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  line-height: 1.6;
  color: #333;
  background: #f5f5f5;
  min-height: 100vh;
}

.container {
  max-width: 1200px;
  margin: 0 auto;
  padding: 20px;
}

/* Utility Classes */
.flex { display: flex; }
.flex-col { flex-direction: column; }
.items-center { align-items: center; }
.justify-center { justify-content: center; }
.gap-2 { gap: 1rem; }
.gap-4 { gap: 2rem; }

/* Button Styles */
button, .btn {
  padding: 10px 20px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 1rem;
  font-weight: 500;
  transition: all 0.2s ease;
  background: #3b82f6;
  color: white;
}

button:hover {
  background: #2563eb;
}

/* Input Styles */
input, textarea {
  padding: 10px 14px;
  border: 1px solid #ddd;
  border-radius: 6px;
  font-size: 1rem;
  width: 100%;
}

input:focus, textarea:focus {
  outline: none;
  border-color: #3b82f6;
}

/* Card Styles */
.card {
  background: white;
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.1);
}

/* APP_STYLES: Add your custom styles below */
`
    },
    {
      path: 'app/page.js',
      description: 'Main page component - build your UI here',
      content: `'use client'

import { useState, useEffect } from 'react'

export default function Home() {
  // APP_STATE: Add your state hooks here
  const [data, setData] = useState(null)
  const [loading, setLoading] = useState(true)

  // APP_EFFECTS: Fetch data on mount
  useEffect(() => {
    fetchData()
  }, [])

  async function fetchData() {
    try {
      const res = await fetch('/api/data')
      const json = await res.json()
      setData(json)
    } catch (error) {
      console.error('Error fetching data:', error)
    } finally {
      setLoading(false)
    }
  }

  // APP_HANDLERS: Add your event handlers here

  if (loading) return <div className="container"><p>Loading...</p></div>

  return (
    <main className="container">
      <h1>{{APP_TITLE}}</h1>
      {/* APP_CONTENT: Build your UI here */}
    </main>
  )
}`
    },
    {
      path: 'app/api/data/route.js',
      description: 'API route - handles GET/POST requests',
      content: `import { NextResponse } from 'next/server'

// In-memory data store (replace with database in production)
// This persists during the server process lifetime
let store = {
  // APP_DATA: Define your initial data structure here
  items: [],
  users: {}
}

// GET - Fetch data
export async function GET() {
  return NextResponse.json(store)
}

// POST - Create/update data
export async function POST(request) {
  try {
    const body = await request.json()

    // APP_POST_LOGIC: Handle POST requests here
    // Example: Add item to store
    // store.items.push({ id: Date.now(), ...body })

    return NextResponse.json({ success: true, data: store })
  } catch (error) {
    return NextResponse.json({ error: error.message }, { status: 400 })
  }
}`
    }
  ],
  instructions: `
When using this scaffold:
1. Replace {{APP_NAME}}, {{APP_TITLE}}, {{APP_DESCRIPTION}} with actual values
2. Build your UI in app/page.js
3. Add API logic in app/api/data/route.js (or create more API routes)
4. Add styles in app/globals.css
5. After creating files, run: shell("npm install")
6. Tell user to run: npm run dev
7. The app will be available at http://localhost:3000
`
};

/**
 * Node.js Express API scaffold (for API-only backends)
 */
export const nodeExpressApi: Scaffold = {
  name: 'node-express-api',
  description: 'Node.js REST API with Express (API only, no frontend)',
  keywords: ['express', 'api-only', 'backend-only', 'microservice'],
  files: [
    {
      path: 'package.json',
      description: 'Dependencies',
      content: `{
  "name": "{{APP_NAME}}",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "start": "node server.js",
    "dev": "node --watch server.js"
  },
  "dependencies": {
    "express": "^4.18.2",
    "cors": "^2.8.5"
  }
}`
    },
    {
      path: 'server.js',
      description: 'Express server - add your routes here',
      content: `import express from 'express';
import cors from 'cors';

const app = express();
const PORT = process.env.PORT || 3000;

// Middleware
app.use(cors());
app.use(express.json());
app.use(express.static('public')); // Serve static files from /public

// In-memory data store (replace with database in production)
let data = {
  // APP_DATA: Define your initial data here
};

// ============================================
// ROUTES
// ============================================

// Health check
app.get('/health', (req, res) => {
  res.json({ status: 'ok', timestamp: new Date().toISOString() });
});

// APP_ROUTES: Add your API routes here

// Error handling
app.use((err, req, res, next) => {
  console.error(err.stack);
  res.status(500).json({ error: 'Something went wrong!' });
});

// Start server
app.listen(PORT, () => {
  console.log(\`Server running on http://localhost:\${PORT}\`);
});
`
    },
    {
      path: 'public/index.html',
      description: 'Frontend HTML (served by Express)',
      content: `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{{APP_TITLE}}</title>
    <link rel="stylesheet" href="styles.css">
</head>
<body>
    <div id="app">
        <!-- APP_CONTENT: Add your HTML here -->
    </div>
    <script src="app.js"></script>
</body>
</html>`
    },
    {
      path: 'public/styles.css',
      description: 'Frontend styles',
      content: `/* CSS Reset */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  line-height: 1.6;
  color: #333;
  background: #f5f5f5;
  min-height: 100vh;
}

#app {
  max-width: 1200px;
  margin: 0 auto;
  padding: 20px;
}

/* APP_STYLES: Add your custom styles below */
`
    },
    {
      path: 'public/app.js',
      description: 'Frontend JavaScript',
      content: `// API helper
async function api(endpoint, method = 'GET', body = null) {
  const options = { method, headers: { 'Content-Type': 'application/json' } };
  if (body) options.body = JSON.stringify(body);
  const res = await fetch(endpoint, options);
  return res.json();
}

// APP_STATE: Define your state here
let state = {};

// APP_FUNCTIONS: Add your functions here

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
  // Load initial data
  // const data = await api('/api/data');
  // APP_INIT: Initialize your app here
});
`
    }
  ],
  instructions: `
When using this scaffold:
1. Replace {{APP_NAME}} and {{APP_TITLE}} with actual values
2. Add API routes in server.js
3. Build frontend in public/index.html, public/styles.css, public/app.js
4. After creating files, run: shell("npm install")
5. Tell user to run: npm run dev
`
};

/**
 * Python FastAPI scaffold
 */
export const pythonFastApi: Scaffold = {
  name: 'python-fastapi',
  description: 'Python REST API with FastAPI',
  keywords: ['python', 'api', 'fastapi', 'backend', 'rest'],
  files: [
    {
      path: 'requirements.txt',
      description: 'Python dependencies',
      content: `fastapi>=0.104.0
uvicorn[standard]>=0.24.0
pydantic>=2.5.0`
    },
    {
      path: 'main.py',
      description: 'FastAPI server - add your routes here',
      content: `from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import List, Optional
from datetime import datetime

app = FastAPI(title="{{APP_TITLE}}")

# CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# ============================================
# MODELS
# ============================================

# APP_MODELS: Define your Pydantic models here
# Example:
# class Item(BaseModel):
#     id: Optional[int] = None
#     name: str
#     description: Optional[str] = None

# ============================================
# IN-MEMORY DATA
# ============================================

# APP_DATA: Define your data store here
# Example: items: List[dict] = []

# ============================================
# ROUTES
# ============================================

@app.get("/health")
def health_check():
    return {"status": "ok", "timestamp": datetime.now().isoformat()}

# APP_ROUTES: Add your API routes here
# Example:
# @app.get("/api/items")
# def get_items():
#     return items
#
# @app.post("/api/items")
# def create_item(item: Item):
#     item.id = len(items) + 1
#     items.append(item.dict())
#     return item

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
`
    }
  ],
  instructions: `
When using this scaffold:
1. Replace {{APP_TITLE}} with the actual app name
2. Define Pydantic models in APP_MODELS section
3. Add routes in APP_ROUTES section
4. After creating files, run: shell("pip install -r requirements.txt")
5. Tell user to run: python main.py (or uvicorn main:app --reload)
`
};

/**
 * All available scaffolds
 */
export const scaffolds: Scaffold[] = [
  vanillaWebApp,
  reactViteApp,
  nextJsApp,
  nodeExpressApi,
  pythonFastApi,
];

/**
 * Detect which scaffold best matches the user's request
 */
export function detectScaffold(userRequest: string): Scaffold | null {
  const request = userRequest.toLowerCase();

  // Check for explicit framework mentions first (highest priority)
  if (request.includes('next') || request.includes('nextjs') || request.includes('next.js')) {
    return nextJsApp;
  }

  if (request.includes('react') || request.includes('jsx') || request.includes('component')) {
    return reactViteApp;
  }

  if (request.includes('express')) {
    return nodeExpressApi;
  }

  if (request.includes('fastapi') || (request.includes('python') && (request.includes('api') || request.includes('server')))) {
    return pythonFastApi;
  }

  // Check for features that imply needing a full-stack app (BEFORE checking for simple apps)
  // These features typically need server-side persistence or authentication
  const fullstackIndicators = [
    'username', 'login', 'register', 'account', 'auth', 'authentication',
    'leaderboard', 'high score', 'scoreboard', 'ranking', 'scores',
    'database', 'persist', 'mongodb', 'postgres', 'mysql',
    'multiplayer', 'real-time', 'realtime', 'websocket',
    'api', 'endpoint', 'backend', 'server'
  ];

  // Count how many fullstack indicators are present
  const fullstackMatches = fullstackIndicators.filter(indicator => request.includes(indicator)).length;

  // If fullstack indicators found, use Next.js (best for full-stack apps)
  if (fullstackMatches >= 1) {
    return nextJsApp;
  }

  // For general web apps, games, simple apps - use vanilla
  // Only if NO fullstack indicators were found
  const vanillaKeywords = ['game', 'todo', 'calculator', 'timer', 'clock', 'counter', 'quiz',
                          'form', 'landing', 'portfolio', 'simple', 'basic', 'html', 'website'];
  if (vanillaKeywords.some(kw => request.includes(kw))) {
    return vanillaWebApp;
  }

  // Default to vanilla web app for any "build" or "create" request
  if (request.includes('build') || request.includes('create') || request.includes('make')) {
    return vanillaWebApp;
  }

  return null;
}

/**
 * Generate scaffold prompt to inject into the model's context
 */
export function generateScaffoldPrompt(scaffold: Scaffold, userRequest: string): string {
  const fileList = scaffold.files.map(f => `- ${f.path}: ${f.description}`).join('\n');

  const fileContents = scaffold.files.map(f =>
    `### ${f.path}\n\`\`\`\n${f.content}\n\`\`\``
  ).join('\n\n');

  return `
## SCAFFOLD: ${scaffold.name}

I've selected the "${scaffold.description}" scaffold for this request.

### Files to create:
${fileList}

### Template Contents:
${fileContents}

### Instructions:
${scaffold.instructions}

### Your Task:
Using the scaffold above as a starting point, create a complete ${userRequest}.
- Use file_write to create each file
- Fill in the placeholder sections (marked with APP_*, {{VARIABLE}})
- Add the specific functionality the user requested
- Make sure all files work together as a complete application
`;
}
