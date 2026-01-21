// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

/**
 * Docker container manager for Teddy
 * Manages PostgreSQL and other service containers
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { mkdir, writeFile, readFile } from 'fs/promises';
import { existsSync } from 'fs';
import path from 'path';
import { app } from 'electron';

const execAsync = promisify(exec);

export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  status: 'running' | 'exited' | 'created' | 'paused' | 'unknown';
  ports: Record<string, number>;
  created: string;
}

export interface ServiceConfig {
  name: string;
  image: string;
  containerId?: string;
  status: 'running' | 'stopped' | 'unknown';
  ports: Record<string, number>;
  volumes: Record<string, string>;
  env: Record<string, string>;
  created?: number;
  lastStarted?: number;
}

export interface PostgresConfig {
  password: string;
  port: number;
  database: string;
  user: string;
}

// Default PostgreSQL configuration
const DEFAULT_POSTGRES_CONFIG: PostgresConfig = {
  password: 'teddy-dev-password',
  port: 5432,
  database: 'teddy_dev',
  user: 'postgres',
};

/**
 * Get the Teddy data directory for Docker volumes
 */
function getDataDir(): string {
  return path.join(app.getPath('userData'), 'docker');
}

/**
 * Get the services registry file path
 */
function getServicesPath(): string {
  return path.join(getDataDir(), 'services.json');
}

/**
 * Load services registry from disk
 */
async function loadServices(): Promise<Record<string, ServiceConfig>> {
  const servicesPath = getServicesPath();
  if (!existsSync(servicesPath)) {
    return {};
  }
  try {
    const content = await readFile(servicesPath, 'utf-8');
    return JSON.parse(content);
  } catch {
    return {};
  }
}

/**
 * Save services registry to disk
 */
async function saveServices(services: Record<string, ServiceConfig>): Promise<void> {
  const dataDir = getDataDir();
  await mkdir(dataDir, { recursive: true });
  await writeFile(getServicesPath(), JSON.stringify(services, null, 2));
}

/**
 * Get PostgreSQL data directory
 */
function getPostgresDataDir(): string {
  return path.join(getDataDir(), 'postgres-data');
}

/**
 * Build DATABASE_URL from PostgreSQL config
 */
export function buildDatabaseUrl(config: PostgresConfig = DEFAULT_POSTGRES_CONFIG): string {
  return `postgresql://${config.user}:${config.password}@localhost:${config.port}/${config.database}`;
}

/**
 * Start a PostgreSQL container
 */
export async function startPostgresContainer(config: Partial<PostgresConfig> = {}): Promise<{
  success: boolean;
  containerId?: string;
  databaseUrl?: string;
  error?: string;
}> {
  const fullConfig = { ...DEFAULT_POSTGRES_CONFIG, ...config };
  const dataDir = getPostgresDataDir();

  // Ensure data directory exists
  await mkdir(dataDir, { recursive: true });

  // Check if container already exists
  const services = await loadServices();
  if (services['postgres']?.status === 'running') {
    return {
      success: true,
      containerId: services['postgres'].containerId,
      databaseUrl: buildDatabaseUrl(fullConfig),
    };
  }

  // Try to start existing container first
  if (services['postgres']?.containerId) {
    try {
      await execAsync(`docker start ${services['postgres'].containerId}`);
      services['postgres'].status = 'running';
      services['postgres'].lastStarted = Date.now();
      await saveServices(services);
      return {
        success: true,
        containerId: services['postgres'].containerId,
        databaseUrl: buildDatabaseUrl(fullConfig),
      };
    } catch {
      // Container doesn't exist anymore, will create new one
    }
  }

  // Create and start new container
  const containerName = 'teddy-postgres';
  const command = [
    'docker run -d',
    `--name ${containerName}`,
    `-e POSTGRES_PASSWORD=${fullConfig.password}`,
    `-e POSTGRES_USER=${fullConfig.user}`,
    `-e POSTGRES_DB=${fullConfig.database}`,
    `-p ${fullConfig.port}:5432`,
    `-v "${dataDir}:/var/lib/postgresql/data"`,
    '--restart unless-stopped',
    'postgres:15-alpine',
  ].join(' ');

  try {
    // Remove existing container with same name if it exists
    try {
      await execAsync(`docker rm -f ${containerName}`);
    } catch {
      // Container doesn't exist, that's fine
    }

    const { stdout } = await execAsync(command);
    const containerId = stdout.trim();

    // Update services registry
    services['postgres'] = {
      name: 'postgres',
      image: 'postgres:15-alpine',
      containerId,
      status: 'running',
      ports: { '5432': fullConfig.port },
      volumes: { '/var/lib/postgresql/data': dataDir },
      env: {
        POSTGRES_USER: fullConfig.user,
        POSTGRES_DB: fullConfig.database,
      },
      created: Date.now(),
      lastStarted: Date.now(),
    };
    await saveServices(services);

    // Wait for PostgreSQL to be ready (up to 30 seconds)
    const isReady = await waitForPostgres(fullConfig, 30000);
    if (!isReady) {
      return {
        success: true,
        containerId,
        databaseUrl: buildDatabaseUrl(fullConfig),
        error: 'Container started but PostgreSQL may not be fully ready yet. Try again in a few seconds.',
      };
    }

    return {
      success: true,
      containerId,
      databaseUrl: buildDatabaseUrl(fullConfig),
    };
  } catch (err) {
    return {
      success: false,
      error: `Failed to start PostgreSQL container: ${err}`,
    };
  }
}

/**
 * Wait for PostgreSQL to be ready to accept connections
 */
async function waitForPostgres(config: PostgresConfig, timeoutMs: number): Promise<boolean> {
  const startTime = Date.now();
  const checkInterval = 1000; // 1 second

  while (Date.now() - startTime < timeoutMs) {
    try {
      // Try to connect using psql inside the container
      await execAsync(
        `docker exec teddy-postgres pg_isready -U ${config.user} -d ${config.database}`,
        { timeout: 5000 }
      );
      return true;
    } catch {
      // Not ready yet, wait and retry
      await new Promise(resolve => setTimeout(resolve, checkInterval));
    }
  }

  return false;
}

/**
 * Stop the PostgreSQL container
 */
export async function stopPostgresContainer(): Promise<{
  success: boolean;
  error?: string;
}> {
  const services = await loadServices();
  const postgres = services['postgres'];

  if (!postgres?.containerId) {
    return { success: true }; // Nothing to stop
  }

  try {
    await execAsync(`docker stop ${postgres.containerId}`);
    postgres.status = 'stopped';
    await saveServices(services);
    return { success: true };
  } catch (err) {
    return {
      success: false,
      error: `Failed to stop PostgreSQL container: ${err}`,
    };
  }
}

/**
 * Remove the PostgreSQL container (preserves data)
 */
export async function removePostgresContainer(): Promise<{
  success: boolean;
  error?: string;
}> {
  const services = await loadServices();
  const postgres = services['postgres'];

  if (!postgres?.containerId) {
    return { success: true }; // Nothing to remove
  }

  try {
    await execAsync(`docker rm -f ${postgres.containerId}`);
    delete services['postgres'];
    await saveServices(services);
    return { success: true };
  } catch (err) {
    return {
      success: false,
      error: `Failed to remove PostgreSQL container: ${err}`,
    };
  }
}

/**
 * Get PostgreSQL container status
 */
export async function getPostgresStatus(): Promise<{
  installed: boolean;
  running: boolean;
  containerId: string | null;
  databaseUrl: string | null;
  port: number;
  dataDir: string;
}> {
  const services = await loadServices();
  const postgres = services['postgres'];

  if (!postgres?.containerId) {
    return {
      installed: false,
      running: false,
      containerId: null,
      databaseUrl: null,
      port: DEFAULT_POSTGRES_CONFIG.port,
      dataDir: getPostgresDataDir(),
    };
  }

  // Check actual container status
  let isRunning = false;
  try {
    const { stdout } = await execAsync(
      `docker inspect --format='{{.State.Running}}' ${postgres.containerId}`
    );
    isRunning = stdout.trim() === 'true';

    // Update services registry if status changed
    if (isRunning && postgres.status !== 'running') {
      postgres.status = 'running';
      await saveServices(services);
    } else if (!isRunning && postgres.status !== 'stopped') {
      postgres.status = 'stopped';
      await saveServices(services);
    }
  } catch {
    // Container doesn't exist anymore
    postgres.status = 'stopped';
    await saveServices(services);
  }

  return {
    installed: true,
    running: isRunning,
    containerId: postgres.containerId,
    databaseUrl: isRunning ? buildDatabaseUrl() : null,
    port: postgres.ports['5432'] || DEFAULT_POSTGRES_CONFIG.port,
    dataDir: getPostgresDataDir(),
  };
}

/**
 * Get PostgreSQL container logs
 */
export async function getPostgresLogs(lines: number = 50): Promise<string> {
  const services = await loadServices();
  const postgres = services['postgres'];

  if (!postgres?.containerId) {
    return 'No PostgreSQL container found';
  }

  try {
    const { stdout } = await execAsync(`docker logs --tail ${lines} ${postgres.containerId}`);
    return stdout;
  } catch (err) {
    return `Failed to get logs: ${err}`;
  }
}

/**
 * Execute a SQL query against PostgreSQL using psql in the container
 */
export async function executePostgresQuery(
  query: string,
  config: Partial<PostgresConfig> = {}
): Promise<{
  success: boolean;
  result?: string;
  error?: string;
}> {
  const fullConfig = { ...DEFAULT_POSTGRES_CONFIG, ...config };
  const services = await loadServices();
  const postgres = services['postgres'];

  if (!postgres?.containerId) {
    return {
      success: false,
      error: 'PostgreSQL container is not running. Start it from Settings → Database & Services.',
    };
  }

  // Check if container is running
  const status = await getPostgresStatus();
  if (!status.running) {
    return {
      success: false,
      error: 'PostgreSQL container is not running. Start it from Settings → Database & Services.',
    };
  }

  try {
    // Execute query using psql inside the container
    const escapedQuery = query.replace(/'/g, "'\\''");
    const { stdout, stderr } = await execAsync(
      `docker exec teddy-postgres psql -U ${fullConfig.user} -d ${fullConfig.database} -c '${escapedQuery}'`,
      { timeout: 30000 }
    );

    if (stderr && !stderr.includes('NOTICE')) {
      return {
        success: false,
        error: stderr,
      };
    }

    return {
      success: true,
      result: stdout,
    };
  } catch (err: any) {
    return {
      success: false,
      error: err.message || String(err),
    };
  }
}

/**
 * Test PostgreSQL connection
 */
export async function testPostgresConnection(config: Partial<PostgresConfig> = {}): Promise<{
  success: boolean;
  error?: string;
}> {
  const result = await executePostgresQuery('SELECT 1', config);
  return {
    success: result.success,
    error: result.error,
  };
}
