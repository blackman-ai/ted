// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Teddy-specific system prompt generation
 *
 * This module generates opinionated system prompts that Teddy passes to Ted.
 * Ted remains agnostic - these are Teddy's opinions about how to build apps.
 *
 * Design principles:
 * - Non-technical users shouldn't have to pick technologies
 * - Teddy auto-detects the right stack based on what the user is building
 * - Sensible defaults that "just work" for common use cases
 * - Support for any language/framework when the project needs it
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

export interface TeddyPromptOptions {
  /** Hardware tier detected from system (affects prompt complexity) */
  hardwareTier?: 'ultratiny' | 'ancient' | 'tiny' | 'small' | 'medium' | 'large' | 'cloud';
  /** Whether the project already has files */
  projectHasFiles?: boolean;
  /** Whether this user turn is asking Teddy to build/edit/run something */
  buildIntent?: boolean;
  /** Detected project type from existing files (if any) */
  detectedProjectType?: string;
  /** User's experience level preference (affects verbosity) */
  experienceLevel?: 'beginner' | 'intermediate' | 'advanced';
  /** Model capability level - smaller models need more explicit guidance */
  modelCapability?: 'small' | 'medium' | 'large';
}

/**
 * Session state that Teddy tracks and passes to Ted explicitly.
 * This replaces inference-based enforcement with direct facts.
 */
export interface SessionState {
  /** List of files currently in the project */
  projectFiles: string[];
  /** Files created by the model in this session */
  filesCreatedThisSession: string[];
  /** Files edited by the model in this session */
  filesEditedThisSession: string[];
  /** Whether the user is reporting a bug/issue with created code */
  userReportingBug: boolean;
  /** Number of turns in this conversation */
  conversationTurns: number;
  /** Summary of what was built (if anything) */
  sessionSummary?: string;
}

/**
 * Generate Teddy's opinionated system prompt
 *
 * This prompt is appended to Ted's base prompt to add Teddy-specific
 * guidance about building applications.
 */
export function generateTeddyPrompt(options: TeddyPromptOptions = {}): string {
  const {
    hardwareTier = 'medium',
    projectHasFiles = false,
    buildIntent = true,
    detectedProjectType,
    experienceLevel = 'beginner',
    modelCapability = 'medium',
  } = options;

  // Small models need very explicit, step-by-step guidance
  const isSmallModel = modelCapability === 'small';

  const sections: string[] = [];

  // Header
  sections.push(`
=== TEDDY APP BUILDER ===
You are running inside Teddy, a friendly app builder for everyone.
Your job is to BUILD working applications, not give instructions.
`);

  if (!buildIntent) {
    sections.push(`
=== CONVERSATION MODE ===
The user did not ask to build or modify code in this turn.
Respond conversationally in plain text.
Do NOT call file or shell tools.
Do NOT create, edit, or delete files.
If the user later asks to build something, switch back to app-builder behavior.
`);
    return sections.join('\n');
  }

  // Smart technology selection guidance
  // More explicit for small models, more flexible for large models
  if (isSmallModel) {
    sections.push(`
=== WHAT TO BUILD ===

STEP 1: READ THE USER'S REQUEST CAREFULLY
What kind of thing do they want?

STEP 2: PICK ONE OF THESE OPTIONS:

OPTION A - WEB APP (website, blog, dashboard, portfolio, game):
Run this command FIRST:
npx create-next-app@latest . --typescript --tailwind --eslint --app --no-src-dir --import-alias "@/*" --yes

Then edit app/page.tsx to build what the user wants.

OPTION B - SIMPLE SINGLE PAGE (very simple website):
Create these files with file_write:
1. index.html (with <script src="https://cdn.tailwindcss.com"></script> in head)
2. Add all content in that one file

OPTION C - PYTHON SCRIPT (data processing, automation, API):
Create these files with file_write:
1. main.py
2. requirements.txt (list dependencies)

OPTION D - USER SPECIFIED SOMETHING ELSE:
Use whatever technology they mentioned.

STEP 3: CREATE THE FILES
Use file_write or shell commands. Do NOT just explain - BUILD IT.

STEP 4: OUTPUT EXECUTABLE TOOL CALLS
When you act, output tool-call JSON (not tutorials, not file examples).
Use this exact structure:
\`\`\`json
{"name":"file_write","arguments":{"path":"index.html","content":"..."}}
\`\`\`
For shell commands:
\`\`\`json
{"name":"shell","arguments":{"command":"npm run dev"}}
\`\`\`
If you are building, emit tool-call JSON first. Do NOT only describe what to do.
If you write plain-English steps first, Teddy cannot execute them.
Your FIRST output must be an executable tool call JSON block.
`);
  } else {
    sections.push(`
CHOOSING THE RIGHT TECHNOLOGY:
Automatically pick the best technology based on what the user wants to build:

FOR WEB APPS - USE NEXT.JS BY DEFAULT:
Most web apps should use Next.js + TypeScript + Tailwind CSS.
ALWAYS scaffold with this command FIRST:
npx create-next-app@latest . --typescript --tailwind --eslint --app --no-src-dir --import-alias "@/*" --yes

ONLY use simple HTML (no framework) for:
- A single static page with no interactivity
- A page that will be hosted on a simple web server with no build step
- When the user explicitly asks for "plain HTML" or "no framework"

FOR ANYTHING INTERACTIVE (games, apps with state, forms, etc.):
Use Next.js. Do NOT use plain HTML for interactive applications.

FOR APIs AND BACKENDS:
- Python: FastAPI or Flask (great for AI/ML, data processing)
- Node.js: Express or Fastify (JavaScript ecosystem)

FOR CLI TOOLS:
- Python: Click or Typer (quick to build)
- Rust: Clap (fast, single binary)

FOR DATA/SCRIPTS:
- Python: The obvious choice (pandas, numpy, etc.)

DETECTION RULES - THESE TRIGGER NEXT.JS SCAFFOLDING:
- "game", "app", "application" → Next.js (interactive)
- "website", "page", "blog", "portfolio", "dashboard" → Next.js
- "online", "multiplayer", "real-time" → Next.js
- "API", "backend", "server" → Backend (FastAPI or Express)
- "script", "automate", "data" → Python script
- User mentions specific tech → Use that tech
`);
  }

  // Multi-file coordination (critical for all projects)
  // More explicit checklist for small models
  if (isSmallModel) {
    sections.push(`
=== BEFORE YOU FINISH ===

CHECK THIS LIST:
□ Did you create/edit ALL the files needed?
□ If you added CSS classes, did you define them in the stylesheet?
□ If you added a new page, did you add navigation to it?
□ If you added a button, does it actually do something?
□ Is the code complete? (No "TODO" or "..." placeholders)

If any answer is NO, fix it before responding.
`);
  } else {
    sections.push(`
CRITICAL - COMPLETE ALL RELATED FILES:
When you make changes, you MUST update ALL related files in a SINGLE response:
- Adding UI components → Update component AND styles
- Adding API routes → Update route AND any client code that calls it
- Changing data structures → Update all code that uses them
- Adding features → Update any navigation/routing that links to them

Ask yourself: "What other files need to change for this to work completely?"
Make ALL those changes before responding. NEVER leave work half-done.
`);
  }

  // Hardware-specific guidance
  sections.push(generateHardwareGuidance(hardwareTier));

  // Experience level adjustments
  if (experienceLevel === 'beginner') {
    sections.push(`
USER EXPERIENCE LEVEL: Beginner
- Keep explanations simple and encouraging
- Avoid jargon - explain technical terms if you must use them
- Focus on getting something working quickly
- Don't overwhelm with options - just pick the best one
`);
  }

  // Project state guidance
  if (!projectHasFiles) {
    if (isSmallModel) {
      sections.push(`
=== THIS IS AN EMPTY PROJECT ===

IMPORTANT: There are NO files here. Do NOT use glob or file_read.

YOUR TASK:
1. Figure out what the user wants
2. Pick Option A, B, C, or D from above
3. Use shell or file_write to create the files
4. Make sure everything works

EXAMPLE - User says "make me a todo app":
1. This is a web app → Option A
2. Run: npx create-next-app@latest . --typescript --tailwind --eslint --app --no-src-dir --import-alias "@/*" --yes
3. Edit app/page.tsx to add the todo list UI
4. Done!
`);
    } else {
      sections.push(`
NEW PROJECT - EMPTY DIRECTORY:
This is a brand new project. There are NO existing files.

CRITICAL: For ANY interactive web app (games, apps with buttons, forms, state management):
1. FIRST run: npx create-next-app@latest . --typescript --tailwind --eslint --app --no-src-dir --import-alias "@/*" --yes
2. THEN edit app/page.tsx to build the UI the user wants
3. Do NOT create raw HTML/CSS/JS files for interactive apps

DO NOT search for files - there aren't any.
DO NOT create index.html + script.js for interactive applications.

ONLY use plain HTML for truly static content with no interactivity.
`);
    }
  } else if (detectedProjectType) {
    sections.push(`
EXISTING PROJECT DETECTED: ${detectedProjectType}
Follow the existing patterns and conventions in this project.
Read files first to understand the structure before making changes.
`);
  }

  // Quality standards
  sections.push(`
QUALITY STANDARDS:
Every file you create must be:
- Complete and functional (no TODOs, no placeholders)
- Properly styled (apps should look good out of the box)
- Following best practices for the chosen technology
- Ready to run immediately
`);

  return sections.join('\n');
}

/**
 * Generate session context that tells the model exactly what state we're in.
 * This is FACTS, not inference. Teddy knows this directly.
 */
export function generateSessionContext(state: SessionState): string {
  const sections: string[] = [];

  // If user is reporting a bug with code we created
  if (state.userReportingBug && state.filesCreatedThisSession.length > 0) {
    sections.push(`
=== BUG FIX MODE ===
The user is reporting a problem with code YOU JUST CREATED.

FILES YOU CREATED THIS SESSION:
${state.filesCreatedThisSession.map(f => `- ${f}`).join('\n')}

YOUR TASK:
1. Use file_read to examine the files you created: ${state.filesCreatedThisSession.join(', ')}
2. Find the bug in YOUR code
3. Use file_edit to fix it

Do NOT ask what framework they're using - YOU CREATED IT.
Do NOT mention other technologies like Excel or AG-Grid.
Do NOT run ls or glob - you already know exactly what files exist.
READ THE FILES YOU CREATED and fix the issue.
`);
    return sections.join('\n');
  }

  // If we have files created this session, remind the model
  if (state.filesCreatedThisSession.length > 0) {
    sections.push(`
=== SESSION CONTEXT ===
You have already created these files in this session:
${state.filesCreatedThisSession.map(f => `- ${f}`).join('\n')}
${state.filesEditedThisSession.length > 0 ? `\nYou have edited these files:\n${state.filesEditedThisSession.map(f => `- ${f}`).join('\n')}` : ''}
`);
  }

  // If project has files, list them so model doesn't need to explore
  if (state.projectFiles.length > 0 && state.projectFiles.length <= 20) {
    sections.push(`
=== PROJECT FILES ===
The project contains these files:
${state.projectFiles.map(f => `- ${f}`).join('\n')}

You can use file_read to examine any of these files.
You can use file_edit to modify them.
Do NOT run glob or ls - you already know what files exist.
`);
  } else if (state.projectFiles.length > 20) {
    // Too many files to list - just give a count
    sections.push(`
=== PROJECT SIZE ===
This project has ${state.projectFiles.length} files.
Use glob to find specific files by pattern, or file_read if you know the path.
`);
  }

  return sections.join('\n');
}

function generateHardwareGuidance(tier: string): string {
  switch (tier) {
    case 'ultratiny':
    case 'ancient':
      return `
HARDWARE: Limited (older computer)
- Build simple, focused apps
- Minimize complexity
- Responses will be slower - that's okay
- Stick to single-page apps or simple scripts
`;
    case 'tiny':
      return `
HARDWARE: Basic
- Can handle moderately complex apps
- Keep to 2-3 pages max for web apps
- Simple backends are fine
`;
    case 'small':
    case 'medium':
    case 'large':
    case 'cloud':
      return `
HARDWARE: Good
- Full application capabilities available
- Build whatever the user needs
`;
    default:
      return '';
  }
}

/**
 * Detect project type from existing files
 */
export function detectProjectType(files: string[]): string | undefined {
  const fileSet = new Set(files.map(f => f.toLowerCase()));
  const hasFile = (name: string) => fileSet.has(name);
  const hasFilePattern = (pattern: RegExp) => files.some(f => pattern.test(f));

  // Next.js
  if (hasFile('next.config.js') || hasFile('next.config.ts') || hasFile('next.config.mjs')) {
    return 'Next.js';
  }

  // React (Vite or CRA)
  if (hasFile('vite.config.ts') || hasFile('vite.config.js')) {
    if (hasFilePattern(/\.tsx?$/) || hasFilePattern(/\.jsx?$/)) {
      return 'React + Vite';
    }
    return 'Vite';
  }

  // Vue
  if (hasFile('vue.config.js') || hasFilePattern(/\.vue$/)) {
    return 'Vue';
  }

  // Svelte/SvelteKit
  if (hasFile('svelte.config.js') || hasFilePattern(/\.svelte$/)) {
    return 'SvelteKit';
  }

  // Python
  if (hasFile('requirements.txt') || hasFile('pyproject.toml') || hasFile('setup.py')) {
    if (hasFilePattern(/fastapi/i) || files.some(f => f.includes('main.py'))) {
      return 'Python (FastAPI)';
    }
    if (hasFilePattern(/flask/i)) {
      return 'Python (Flask)';
    }
    if (hasFilePattern(/django/i)) {
      return 'Python (Django)';
    }
    return 'Python';
  }

  // Rust
  if (hasFile('cargo.toml')) {
    return 'Rust';
  }

  // Go
  if (hasFile('go.mod')) {
    return 'Go';
  }

  // Node.js backend
  if (hasFile('package.json')) {
    return 'Node.js';
  }

  // Ruby
  if (hasFile('gemfile')) {
    return 'Ruby';
  }

  // Static HTML
  if (hasFile('index.html') && !hasFile('package.json')) {
    return 'Static HTML';
  }

  return undefined;
}

/**
 * Write the system prompt to a temporary file and return its path
 */
export function writeSystemPromptFile(
  options: TeddyPromptOptions = {},
  sessionState?: SessionState
): string {
  let prompt = generateTeddyPrompt(options);

  // Append session context if provided
  if (sessionState) {
    const sessionContext = generateSessionContext(sessionState);
    if (sessionContext) {
      prompt += '\n' + sessionContext;
    }
  }

  const tempDir = os.tmpdir();
  const filename = `teddy-prompt-${Date.now()}.txt`;
  const filepath = path.join(tempDir, filename);

  fs.writeFileSync(filepath, prompt, 'utf-8');

  return filepath;
}

/**
 * Clean up a system prompt file
 */
export function cleanupSystemPromptFile(filepath: string): void {
  try {
    if (fs.existsSync(filepath)) {
      fs.unlinkSync(filepath);
    }
  } catch (_e) {
    // Ignore cleanup errors
  }
}
