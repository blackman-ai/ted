// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Database tools for Ted
//!
//! Provides tools for initializing, migrating, querying, and seeding SQLite databases
//! using Prisma ORM. Supports SQLite by default with PostgreSQL as an upgrade path.

use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;

use crate::error::Result;
use crate::llm::provider::ToolDefinition;
use crate::tools::{PermissionRequest, SchemaBuilder, Tool, ToolContext, ToolResult};

/// Tool for initializing a database with Prisma
pub struct DatabaseInitTool;

#[async_trait]
impl Tool for DatabaseInitTool {
    fn name(&self) -> &str {
        "database_init"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "database_init".to_string(),
            description: "Initialize a SQLite database with Prisma ORM. Creates the prisma/schema.prisma file and installs dependencies. Use this when starting a new project that needs a database.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("provider", "Database provider: 'sqlite' (default, recommended) or 'postgresql'", false)
                .string("models", "Description of the data models needed (e.g., 'users with name and email, posts with title and content')", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let provider = input["provider"].as_str().unwrap_or("sqlite");
        let models_desc = input["models"].as_str().unwrap_or("");

        // Validate provider
        if provider != "sqlite" && provider != "postgresql" {
            return Ok(ToolResult::error(
                tool_use_id,
                format!(
                    "Invalid provider '{}'. Use 'sqlite' or 'postgresql'.",
                    provider
                ),
            ));
        }

        let prisma_dir = context.working_directory.join("prisma");
        let schema_path = prisma_dir.join("schema.prisma");

        // Check if Prisma is already initialized
        if schema_path.exists() {
            return Ok(ToolResult::error(
                tool_use_id,
                "Prisma is already initialized. Use database_migrate to update the schema, or delete prisma/schema.prisma to reinitialize.",
            ));
        }

        let mut output = String::new();

        // Check if package.json exists
        let package_json = context.working_directory.join("package.json");
        if !package_json.exists() {
            output.push_str("⚠️ No package.json found. Run 'npm init -y' first to initialize a Node.js project.\n\n");
            return Ok(ToolResult::error(tool_use_id, output));
        }

        // Install Prisma dependencies
        output.push_str("📦 Installing Prisma dependencies...\n");

        let install_result = Command::new("npm")
            .args(["install", "prisma", "@prisma/client", "--save-dev"])
            .current_dir(&context.working_directory)
            .stdin(Stdio::null())
            .output()
            .await;

        match install_result {
            Ok(install_output) => {
                if !install_output.status.success() {
                    let stderr = String::from_utf8_lossy(&install_output.stderr);
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!("Failed to install Prisma: {}", stderr),
                    ));
                }
                output.push_str("✅ Prisma dependencies installed\n\n");
            }
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to run npm install: {}. Is npm installed?", e),
                ));
            }
        }

        // Initialize Prisma
        output.push_str("🔧 Initializing Prisma...\n");

        let datasource = if provider == "sqlite" {
            "--datasource-provider sqlite"
        } else {
            "--datasource-provider postgresql"
        };

        let init_result = Command::new("npx")
            .args(["prisma", "init", datasource])
            .current_dir(&context.working_directory)
            .stdin(Stdio::null())
            .output()
            .await;

        match init_result {
            Ok(init_output) => {
                if !init_output.status.success() {
                    let stderr = String::from_utf8_lossy(&init_output.stderr);
                    // Prisma init might "fail" with exit code 1 but still work
                    if !schema_path.exists() {
                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("Failed to initialize Prisma: {}", stderr),
                        ));
                    }
                }
                output.push_str("✅ Prisma initialized\n\n");
            }
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Failed to run npx prisma init: {}", e),
                ));
            }
        }

        // Read the generated schema
        let schema_content = match std::fs::read_to_string(&schema_path) {
            Ok(content) => content,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Prisma initialized but couldn't read schema: {}", e),
                ));
            }
        };

        output.push_str("📄 Created prisma/schema.prisma:\n");
        output.push_str("```prisma\n");
        output.push_str(&schema_content);
        output.push_str("```\n\n");

        // Provide guidance based on provider
        if provider == "sqlite" {
            output.push_str("💡 SQLite database will be created at prisma/dev.db\n");
            output.push_str("   No additional setup required - it just works!\n\n");
        } else {
            output.push_str("💡 PostgreSQL requires a running database server.\n");
            output.push_str("   Set DATABASE_URL in .env to your connection string:\n");
            output.push_str(
                "   DATABASE_URL=\"postgresql://user:password@localhost:5432/dbname\"\n\n",
            );
        }

        // Add guidance for next steps
        output.push_str("📝 Next steps:\n");
        output.push_str("1. Add your data models to prisma/schema.prisma\n");
        output.push_str("2. Run database_migrate to create the database and tables\n");
        output.push_str("3. Use @prisma/client in your code to query the database\n\n");

        if !models_desc.is_empty() {
            output.push_str(&format!("💭 You mentioned needing: {}\n", models_desc));
            output.push_str("   I can help you define these models in the schema. Let me know!\n");
        }

        Ok(ToolResult::success(tool_use_id, output))
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let provider = input["provider"].as_str().unwrap_or("sqlite");
        Some(PermissionRequest {
            tool_name: "database_init".to_string(),
            action_description: format!(
                "Initialize {} database with Prisma (installs dependencies, creates schema)",
                provider
            ),
            affected_paths: vec![
                "prisma/schema.prisma".to_string(),
                "package.json".to_string(),
                "node_modules/".to_string(),
            ],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        true // Modifies package.json and creates files
    }
}

/// Tool for running Prisma migrations
pub struct DatabaseMigrateTool;

#[async_trait]
impl Tool for DatabaseMigrateTool {
    fn name(&self) -> &str {
        "database_migrate"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "database_migrate".to_string(),
            description: "Run Prisma migrations to sync the database schema with your models. Creates the database if it doesn't exist. Use after modifying prisma/schema.prisma.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("name", "Migration name (e.g., 'add_users_table', 'init'). Will be auto-generated if not provided.", false)
                .boolean("reset", "Reset the database and re-run all migrations (WARNING: deletes all data)", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let migration_name = input["name"].as_str();
        let reset = input["reset"].as_bool().unwrap_or(false);

        // Check if Prisma is initialized
        let schema_path = context.working_directory.join("prisma/schema.prisma");
        if !schema_path.exists() {
            return Ok(ToolResult::error(
                tool_use_id,
                "Prisma is not initialized. Run database_init first.",
            ));
        }

        let mut output = String::new();

        if reset {
            output.push_str("⚠️ Resetting database (all data will be deleted)...\n");

            let reset_result = Command::new("npx")
                .args(["prisma", "migrate", "reset", "--force"])
                .current_dir(&context.working_directory)
                .stdin(Stdio::null())
                .output()
                .await;

            match reset_result {
                Ok(reset_output) => {
                    if !reset_output.status.success() {
                        let stderr = String::from_utf8_lossy(&reset_output.stderr);
                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("Failed to reset database: {}", stderr),
                        ));
                    }
                    output.push_str("✅ Database reset complete\n\n");
                }
                Err(e) => {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!("Failed to run prisma migrate reset: {}", e),
                    ));
                }
            }
        } else {
            // Run migration in dev mode
            output.push_str("🔄 Running Prisma migration...\n");

            let mut args = vec!["prisma", "migrate", "dev"];
            if let Some(name) = migration_name {
                args.push("--name");
                args.push(name);
            }

            let migrate_result = Command::new("npx")
                .args(&args)
                .current_dir(&context.working_directory)
                .stdin(Stdio::null())
                .output()
                .await;

            match migrate_result {
                Ok(migrate_output) => {
                    let stdout = String::from_utf8_lossy(&migrate_output.stdout);
                    let stderr = String::from_utf8_lossy(&migrate_output.stderr);

                    if !migrate_output.status.success() {
                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("Migration failed:\n{}\n{}", stdout, stderr),
                        ));
                    }

                    output.push_str("✅ Migration complete\n\n");
                    if !stdout.is_empty() {
                        output.push_str("Output:\n");
                        output.push_str(&stdout);
                        output.push('\n');
                    }
                }
                Err(e) => {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!("Failed to run prisma migrate: {}", e),
                    ));
                }
            }
        }

        // Generate Prisma client
        output.push_str("🔧 Generating Prisma client...\n");

        let generate_result = Command::new("npx")
            .args(["prisma", "generate"])
            .current_dir(&context.working_directory)
            .stdin(Stdio::null())
            .output()
            .await;

        match generate_result {
            Ok(gen_output) => {
                if !gen_output.status.success() {
                    let stderr = String::from_utf8_lossy(&gen_output.stderr);
                    output.push_str(&format!(
                        "⚠️ Warning: Client generation failed: {}\n",
                        stderr
                    ));
                } else {
                    output.push_str("✅ Prisma client generated\n\n");
                }
            }
            Err(e) => {
                output.push_str(&format!("⚠️ Warning: Could not generate client: {}\n", e));
            }
        }

        output.push_str("📝 You can now use the Prisma client in your code:\n");
        output.push_str("```typescript\n");
        output.push_str("import { PrismaClient } from '@prisma/client'\n");
        output.push_str("const prisma = new PrismaClient()\n");
        output.push_str("```\n");

        Ok(ToolResult::success(tool_use_id, output))
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let reset = input["reset"].as_bool().unwrap_or(false);
        Some(PermissionRequest {
            tool_name: "database_migrate".to_string(),
            action_description: if reset {
                "Reset database and re-run all migrations (DELETES ALL DATA)".to_string()
            } else {
                "Run Prisma migration to sync database schema".to_string()
            },
            affected_paths: vec![
                "prisma/migrations/".to_string(),
                "prisma/dev.db".to_string(),
            ],
            is_destructive: reset,
        })
    }

    fn requires_permission(&self) -> bool {
        true // Modifies database
    }
}

/// Tool for querying the database
pub struct DatabaseQueryTool;

#[async_trait]
impl Tool for DatabaseQueryTool {
    fn name(&self) -> &str {
        "database_query"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "database_query".to_string(),
            description: "Execute a SQL query against the database. Read-only queries (SELECT) are safe. Write queries (INSERT, UPDATE, DELETE) require explicit permission.".to_string(),
            input_schema: SchemaBuilder::new()
                .string("query", "SQL query to execute (e.g., 'SELECT * FROM users')", true)
                .boolean("allow_write", "Allow write operations (INSERT, UPDATE, DELETE). Default: false", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let query = input["query"]
            .as_str()
            .ok_or_else(|| crate::error::TedError::InvalidInput("query is required".to_string()))?;
        let allow_write = input["allow_write"].as_bool().unwrap_or(false);

        // Check if this is a write query
        let query_upper = query.to_uppercase();
        let is_write = query_upper.contains("INSERT")
            || query_upper.contains("UPDATE")
            || query_upper.contains("DELETE")
            || query_upper.contains("DROP")
            || query_upper.contains("CREATE")
            || query_upper.contains("ALTER")
            || query_upper.contains("TRUNCATE");

        if is_write && !allow_write {
            return Ok(ToolResult::error(
                tool_use_id,
                "Write operations are not allowed by default. Set allow_write: true to execute INSERT, UPDATE, DELETE, or DDL statements.",
            ));
        }

        // Check if Prisma is initialized
        let schema_path = context.working_directory.join("prisma/schema.prisma");
        if !schema_path.exists() {
            return Ok(ToolResult::error(
                tool_use_id,
                "Prisma is not initialized. Run database_init first.",
            ));
        }

        // Determine database type and path from schema
        let schema_content = match std::fs::read_to_string(&schema_path) {
            Ok(content) => content,
            Err(e) => {
                return Ok(ToolResult::error(
                    tool_use_id,
                    format!("Could not read schema: {}", e),
                ));
            }
        };

        // For SQLite, use sqlite3 directly
        if schema_content.contains("provider = \"sqlite\"")
            || schema_content.contains("provider = 'sqlite'")
        {
            // Find the database file
            let db_path = context.working_directory.join("prisma/dev.db");
            if !db_path.exists() {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "Database file not found. Run database_migrate first to create the database.",
                ));
            }

            // Execute query using sqlite3
            let query_result = Command::new("sqlite3")
                .args([
                    "-header",
                    "-column",
                    db_path.to_string_lossy().as_ref(),
                    query,
                ])
                .current_dir(&context.working_directory)
                .stdin(Stdio::null())
                .output()
                .await;

            match query_result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    if !output.status.success() {
                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("Query failed: {}", stderr),
                        ));
                    }

                    let mut result = String::new();
                    result.push_str(&format!("✅ Query executed: {}\n\n", query));
                    if stdout.is_empty() {
                        result.push_str("(No results)\n");
                    } else {
                        result.push_str(&stdout);
                    }

                    Ok(ToolResult::success(tool_use_id, result))
                }
                Err(e) => {
                    // sqlite3 might not be installed
                    Ok(ToolResult::error(
                        tool_use_id,
                        format!("Failed to execute query: {}. Is sqlite3 installed?", e),
                    ))
                }
            }
        } else {
            // For PostgreSQL, use psql via DATABASE_URL from .env
            let env_path = context.working_directory.join(".env");
            let database_url = if env_path.exists() {
                // Try to read DATABASE_URL from .env
                match std::fs::read_to_string(&env_path) {
                    Ok(content) => content
                        .lines()
                        .find(|line| line.starts_with("DATABASE_URL="))
                        .map(|line| {
                            line.trim_start_matches("DATABASE_URL=")
                                .trim_matches('"')
                                .to_string()
                        }),
                    Err(_) => None,
                }
            } else {
                None
            };

            let database_url = match database_url {
                Some(url) if !url.is_empty() => url,
                _ => {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        "PostgreSQL DATABASE_URL not found in .env file. Set DATABASE_URL=\"postgresql://user:password@localhost:5432/dbname\" in your .env file, or start the PostgreSQL container (ted-postgres).",
                    ));
                }
            };

            // Execute query using psql
            // First check if we're using Docker (ted-postgres container) or external PostgreSQL
            let query_result =
                if database_url.contains("localhost") || database_url.contains("127.0.0.1") {
                    // Try to use psql inside the ted-postgres container first
                    Command::new("docker")
                        .args(["exec", "ted-postgres", "psql", &database_url, "-c", query])
                        .current_dir(&context.working_directory)
                        .stdin(Stdio::null())
                        .output()
                        .await
                } else {
                    // External PostgreSQL - try to use local psql
                    Command::new("psql")
                        .args([&database_url, "-c", query])
                        .current_dir(&context.working_directory)
                        .stdin(Stdio::null())
                        .output()
                        .await
                };

            match query_result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    if !output.status.success() {
                        // If Docker command failed, try local psql as fallback
                        if database_url.contains("localhost") || database_url.contains("127.0.0.1")
                        {
                            let fallback_result = Command::new("psql")
                                .args([&database_url, "-c", query])
                                .current_dir(&context.working_directory)
                                .stdin(Stdio::null())
                                .output()
                                .await;

                            match fallback_result {
                                Ok(fb_output) => {
                                    let fb_stdout = String::from_utf8_lossy(&fb_output.stdout);
                                    let fb_stderr = String::from_utf8_lossy(&fb_output.stderr);

                                    if !fb_output.status.success() {
                                        return Ok(ToolResult::error(
                                            tool_use_id,
                                            format!("PostgreSQL query failed: {}", fb_stderr),
                                        ));
                                    }

                                    let mut result = String::new();
                                    result.push_str(&format!("✅ Query executed: {}\n\n", query));
                                    if fb_stdout.is_empty() {
                                        result.push_str("(No results)\n");
                                    } else {
                                        result.push_str(&fb_stdout);
                                    }
                                    return Ok(ToolResult::success(tool_use_id, result));
                                }
                                Err(e) => {
                                    return Ok(ToolResult::error(
                                        tool_use_id,
                                        format!("PostgreSQL query failed: {}. Neither ted-postgres container nor local psql is available.", e),
                                    ));
                                }
                            }
                        }

                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("PostgreSQL query failed: {}", stderr),
                        ));
                    }

                    let mut result = String::new();
                    result.push_str(&format!("✅ Query executed: {}\n\n", query));
                    if stdout.is_empty() {
                        result.push_str("(No results)\n");
                    } else {
                        result.push_str(&stdout);
                    }

                    Ok(ToolResult::success(tool_use_id, result))
                }
                Err(e) => {
                    // psql might not be installed, try local fallback
                    if database_url.contains("localhost") || database_url.contains("127.0.0.1") {
                        Ok(ToolResult::error(
                            tool_use_id,
                            format!("Failed to execute PostgreSQL query: {}. Ensure the ted-postgres container is running or that psql is installed locally.", e),
                        ))
                    } else {
                        Ok(ToolResult::error(
                            tool_use_id,
                            format!(
                                "Failed to execute PostgreSQL query: {}. Is psql installed?",
                                e
                            ),
                        ))
                    }
                }
            }
        }
    }

    fn permission_request(&self, input: &Value) -> Option<PermissionRequest> {
        let query = input["query"].as_str().unwrap_or("");
        let query_upper = query.to_uppercase();
        let is_write = query_upper.contains("INSERT")
            || query_upper.contains("UPDATE")
            || query_upper.contains("DELETE")
            || query_upper.contains("DROP")
            || query_upper.contains("CREATE")
            || query_upper.contains("ALTER")
            || query_upper.contains("TRUNCATE");

        Some(PermissionRequest {
            tool_name: "database_query".to_string(),
            action_description: format!(
                "Execute {} SQL query: {}",
                if is_write { "WRITE" } else { "read-only" },
                if query.len() > 100 {
                    &query[..100]
                } else {
                    query
                }
            ),
            affected_paths: vec!["prisma/dev.db".to_string()],
            is_destructive: is_write,
        })
    }

    fn requires_permission(&self) -> bool {
        false // Read-only queries don't need permission; writes are blocked by default
    }
}

/// Tool for seeding the database with sample data
pub struct DatabaseSeedTool;

#[async_trait]
impl Tool for DatabaseSeedTool {
    fn name(&self) -> &str {
        "database_seed"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "database_seed".to_string(),
            description: "Run the database seed script to populate the database with sample data. The seed script should be at prisma/seed.ts or prisma/seed.js.".to_string(),
            input_schema: SchemaBuilder::new()
                .boolean("create_default", "Create a default seed script if one doesn't exist", false)
                .build(),
        }
    }

    async fn execute(
        &self,
        tool_use_id: String,
        input: Value,
        context: &ToolContext,
    ) -> Result<ToolResult> {
        let create_default = input["create_default"].as_bool().unwrap_or(false);

        // Check if Prisma is initialized
        let schema_path = context.working_directory.join("prisma/schema.prisma");
        if !schema_path.exists() {
            return Ok(ToolResult::error(
                tool_use_id,
                "Prisma is not initialized. Run database_init first.",
            ));
        }

        // Check for seed script
        let seed_ts = context.working_directory.join("prisma/seed.ts");
        let seed_js = context.working_directory.join("prisma/seed.js");

        let seed_exists = seed_ts.exists() || seed_js.exists();

        if !seed_exists {
            if create_default {
                // Create a default seed script
                let default_seed = r#"import { PrismaClient } from '@prisma/client'

const prisma = new PrismaClient()

async function main() {
  console.log('🌱 Seeding database...')

  // Add your seed data here
  // Example:
  // await prisma.user.create({
  //   data: {
  //     email: 'demo@example.com',
  //     name: 'Demo User',
  //   },
  // })

  console.log('✅ Seeding complete!')
}

main()
  .catch((e) => {
    console.error('❌ Seeding failed:', e)
    process.exit(1)
  })
  .finally(async () => {
    await prisma.$disconnect()
  })
"#;

                match std::fs::write(&seed_ts, default_seed) {
                    Ok(_) => {
                        let mut output = String::new();
                        output.push_str("📝 Created default seed script at prisma/seed.ts\n\n");
                        output.push_str("Edit the seed script to add your sample data, then run database_seed again.\n\n");
                        output.push_str("```typescript\n");
                        output.push_str(default_seed);
                        output.push_str("```\n");
                        return Ok(ToolResult::success(tool_use_id, output));
                    }
                    Err(e) => {
                        return Ok(ToolResult::error(
                            tool_use_id,
                            format!("Failed to create seed script: {}", e),
                        ));
                    }
                }
            } else {
                return Ok(ToolResult::error(
                    tool_use_id,
                    "No seed script found at prisma/seed.ts or prisma/seed.js. Use create_default: true to create one.",
                ));
            }
        }

        // Run the seed script
        let mut output = String::new();
        output.push_str("🌱 Running database seed...\n\n");

        let seed_result = Command::new("npx")
            .args(["prisma", "db", "seed"])
            .current_dir(&context.working_directory)
            .stdin(Stdio::null())
            .output()
            .await;

        match seed_result {
            Ok(seed_output) => {
                let stdout = String::from_utf8_lossy(&seed_output.stdout);
                let stderr = String::from_utf8_lossy(&seed_output.stderr);

                if !seed_output.status.success() {
                    return Ok(ToolResult::error(
                        tool_use_id,
                        format!("Seed failed:\n{}\n{}", stdout, stderr),
                    ));
                }

                output.push_str("✅ Database seeded successfully\n\n");
                if !stdout.is_empty() {
                    output.push_str("Output:\n");
                    output.push_str(&stdout);
                }

                Ok(ToolResult::success(tool_use_id, output))
            }
            Err(e) => Ok(ToolResult::error(
                tool_use_id,
                format!("Failed to run seed: {}", e),
            )),
        }
    }

    fn permission_request(&self, _input: &Value) -> Option<PermissionRequest> {
        Some(PermissionRequest {
            tool_name: "database_seed".to_string(),
            action_description: "Run database seed script to populate sample data".to_string(),
            affected_paths: vec!["prisma/dev.db".to_string()],
            is_destructive: false,
        })
    }

    fn requires_permission(&self) -> bool {
        true // Modifies database
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn create_test_context(temp_dir: &TempDir) -> ToolContext {
        ToolContext::new(
            temp_dir.path().to_path_buf(),
            Some(temp_dir.path().to_path_buf()),
            Uuid::new_v4(),
            true,
        )
    }

    // DatabaseInitTool tests

    #[test]
    fn test_database_init_name() {
        let tool = DatabaseInitTool;
        assert_eq!(tool.name(), "database_init");
    }

    #[test]
    fn test_database_init_definition() {
        let tool = DatabaseInitTool;
        let def = tool.definition();
        assert_eq!(def.name, "database_init");
        assert!(def.description.contains("SQLite"));
        assert!(def.description.contains("Prisma"));
    }

    #[test]
    fn test_database_init_requires_permission() {
        let tool = DatabaseInitTool;
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_database_init_permission_request() {
        let tool = DatabaseInitTool;
        let input = serde_json::json!({"provider": "sqlite"});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "database_init");
        assert!(request.action_description.contains("sqlite"));
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_database_init_no_package_json() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DatabaseInitTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("package.json"));
    }

    #[tokio::test]
    async fn test_database_init_invalid_provider() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("package.json"), "{}").unwrap();

        let tool = DatabaseInitTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"provider": "mysql"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("Invalid provider"));
    }

    #[tokio::test]
    async fn test_database_init_already_exists() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("package.json"), "{}").unwrap();
        std::fs::create_dir(temp_dir.path().join("prisma")).unwrap();
        std::fs::write(temp_dir.path().join("prisma/schema.prisma"), "// existing").unwrap();

        let tool = DatabaseInitTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("already initialized"));
    }

    // DatabaseMigrateTool tests

    #[test]
    fn test_database_migrate_name() {
        let tool = DatabaseMigrateTool;
        assert_eq!(tool.name(), "database_migrate");
    }

    #[test]
    fn test_database_migrate_definition() {
        let tool = DatabaseMigrateTool;
        let def = tool.definition();
        assert_eq!(def.name, "database_migrate");
        assert!(def.description.contains("migration"));
    }

    #[test]
    fn test_database_migrate_requires_permission() {
        let tool = DatabaseMigrateTool;
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_database_migrate_permission_request_normal() {
        let tool = DatabaseMigrateTool;
        let input = serde_json::json!({"name": "add_users"});
        let request = tool.permission_request(&input).unwrap();
        assert!(!request.is_destructive);
    }

    #[test]
    fn test_database_migrate_permission_request_reset() {
        let tool = DatabaseMigrateTool;
        let input = serde_json::json!({"reset": true});
        let request = tool.permission_request(&input).unwrap();
        assert!(request.is_destructive);
        assert!(request.action_description.contains("DELETES ALL DATA"));
    }

    #[tokio::test]
    async fn test_database_migrate_not_initialized() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DatabaseMigrateTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("not initialized"));
    }

    // DatabaseQueryTool tests

    #[test]
    fn test_database_query_name() {
        let tool = DatabaseQueryTool;
        assert_eq!(tool.name(), "database_query");
    }

    #[test]
    fn test_database_query_definition() {
        let tool = DatabaseQueryTool;
        let def = tool.definition();
        assert_eq!(def.name, "database_query");
        assert!(def.description.contains("SQL"));
    }

    #[test]
    fn test_database_query_requires_permission() {
        let tool = DatabaseQueryTool;
        assert!(!tool.requires_permission()); // Read queries don't need permission
    }

    #[test]
    fn test_database_query_permission_request_read() {
        let tool = DatabaseQueryTool;
        let input = serde_json::json!({"query": "SELECT * FROM users"});
        let request = tool.permission_request(&input).unwrap();
        assert!(!request.is_destructive);
        assert!(request.action_description.contains("read-only"));
    }

    #[test]
    fn test_database_query_permission_request_write() {
        let tool = DatabaseQueryTool;
        let input = serde_json::json!({"query": "DELETE FROM users"});
        let request = tool.permission_request(&input).unwrap();
        assert!(request.is_destructive);
        assert!(request.action_description.contains("WRITE"));
    }

    #[tokio::test]
    async fn test_database_query_missing_query() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DatabaseQueryTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await;

        assert!(result.is_err()); // Missing required parameter
    }

    #[tokio::test]
    async fn test_database_query_write_blocked() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join("prisma")).unwrap();
        std::fs::write(
            temp_dir.path().join("prisma/schema.prisma"),
            "generator client {\n  provider = \"prisma-client-js\"\n}\n\ndatasource db {\n  provider = \"sqlite\"\n  url = \"file:./dev.db\"\n}",
        ).unwrap();

        let tool = DatabaseQueryTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"query": "DELETE FROM users"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result
            .output_text()
            .contains("Write operations are not allowed"));
    }

    #[tokio::test]
    async fn test_database_query_not_initialized() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DatabaseQueryTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"query": "SELECT * FROM users"}),
                &context,
            )
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("not initialized"));
    }

    // DatabaseSeedTool tests

    #[test]
    fn test_database_seed_name() {
        let tool = DatabaseSeedTool;
        assert_eq!(tool.name(), "database_seed");
    }

    #[test]
    fn test_database_seed_definition() {
        let tool = DatabaseSeedTool;
        let def = tool.definition();
        assert_eq!(def.name, "database_seed");
        assert!(def.description.contains("seed"));
    }

    #[test]
    fn test_database_seed_requires_permission() {
        let tool = DatabaseSeedTool;
        assert!(tool.requires_permission());
    }

    #[test]
    fn test_database_seed_permission_request() {
        let tool = DatabaseSeedTool;
        let input = serde_json::json!({});
        let request = tool.permission_request(&input).unwrap();
        assert_eq!(request.tool_name, "database_seed");
        assert!(!request.is_destructive);
    }

    #[tokio::test]
    async fn test_database_seed_not_initialized() {
        let temp_dir = TempDir::new().unwrap();
        let tool = DatabaseSeedTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_database_seed_no_script() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join("prisma")).unwrap();
        std::fs::write(temp_dir.path().join("prisma/schema.prisma"), "// schema").unwrap();

        let tool = DatabaseSeedTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute("test-id".to_string(), serde_json::json!({}), &context)
            .await
            .unwrap();

        assert!(result.is_error());
        assert!(result.output_text().contains("No seed script"));
    }

    #[tokio::test]
    async fn test_database_seed_create_default() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::create_dir(temp_dir.path().join("prisma")).unwrap();
        std::fs::write(temp_dir.path().join("prisma/schema.prisma"), "// schema").unwrap();

        let tool = DatabaseSeedTool;
        let context = create_test_context(&temp_dir);

        let result = tool
            .execute(
                "test-id".to_string(),
                serde_json::json!({"create_default": true}),
                &context,
            )
            .await
            .unwrap();

        assert!(!result.is_error());
        assert!(result.output_text().contains("Created default seed script"));
        assert!(temp_dir.path().join("prisma/seed.ts").exists());
    }
}
