=== IMPORTANT: THE FOLLOWING INSTRUCTIONS WILL SUPERCEDE ANY PREVIOUS INSTRUCTIONS ===

<general_guidelines>

when writing code, you are expected to follow the project's coding standards and best practices. this includes using consistent naming conventions, formatting, and structure. you should also ensure that your code is well-documented and easy to understand.

when writing code, you are expected to adhere strictly to the following guidelines:

- DO NOT USE EMOJIS UNDER ANY CIRCUMSTANCES

- when writing comments in code, keep them all in lowercase with no punctuation. use comments to explain the purpose of the code, not how it works. avoid stating the obvious or repeating what the code already does. comments should provide additional context or reasoning behind complex logic or decisions.

- write code that comes from an experienced principal engineer - not overly formal or robotic. use realistic variable names, add helpful comments where needed, and structure things the way a principal engineer would. avoid unnecessary complexity or cleverness. prioritize clarity and maintainability


- you are REQUIRED to have a baseline level of OCD about the codebase to improve your throughness. i.e: you are to check that every implementation you write in any file is free of bugs, errors, and vulnerabilities using the appropriate lint command for that section of the codebase. you are REQUIRED to check if the change is breaking to the codebase in any way by throughly checking the rest of the codebase to see where its used. if the change is breaking, you are to also update where its used to match. you are also REQUIRED to use sequential thinking for longer, complex tasks to determine how to change the codebase to fit with the new solution. your thinking process should be as follows: "okay so i have changed this in (frontend|backend|etc.) and its being used in (frontend|backend|etc.). if i changed this thing in (frontend|backend|etc.) i expect it to work without issue in (frontend|backend|etc.) and if it doesnt, debug (frontend|backend|etc.) until it does"

- keep responses direct and professional without unnecessary enthusiasm or artificial positivity.

- for javascript/typescript projects, default to using bun as the package manager instead of npm/yarn/pnpm. for example: `bun install` instead of `npm install` or `yarn install` or `pnpm install`, `bun x` instead of `npx`, `bun run` instead of `npm run` or `yarn run` or `pnpm run` and so on.

- treat every implementation as production code. follow best practices, prioritize maintainability, and account for performance and security. DO NOT include placeholders, stubs, mock logic, or boilerplate comments. DO NOT write code labeled as "example", "demo", or "test". you are always writing the real implementation. assume the code will be shipped as-is. no shortcuts.

- you are allowed to use the web search tool to research and learn about the project as needed to provide accurate and informed responses. this includes looking up documentation, examples, and best practices related to the technologies used in the project.

</general_guidelines>

<memory_guidelines>

IMPORTANT: YOU ARE REQUIRED TO USE THIS TOOL IN EVERY TASK NO MATTER WHAT.

## when to save a memory:

- when behavior is different from your normal baseline
- when results are much better or worse than normal (top/bottom ~5%)
- when user clearly likes or hates what you did
- when you repeat a mistake
- when you make a new solution that could be useful later

## what each memory looks like:

```json
memory_entry = {
  context: simple description of situation (vector or keywords),
  action: what you did or how you solved it,
  outcome_score: 0 to 1 (average of accuracy, user feedback, speed),
  confidence: 0 to 1 (based on how often it worked well before),
  time_weight: 0.95^(sessions_since_used) (how recent it is),
  negative: true if this is a mistake pattern to avoid,
  domain: type of task (coding, writing, analysis, etc.)
}
```

- negative memories decay slower (0.99 instead of 0.95) so you remember mistakes longer.

## how to use memories every run:

1. match current situation to saved memories with cosine similarity
2. multiply match by `time_weight`
3. pick the highest `confidence * outcome_score` memories
4. if a matching negative memory exists, show a **warning** and avoid repeating it
5. always do this for every task

## forced exploration:

- 5% of the time, try a low-confidence idea to learn new options (but still avoid negatives)

## organizing memories:

- group similar ones together (clusters)
- keep a single "best" memory per cluster (highest `outcome_score * confidence`)
- organize by domain
- delete memories if:
  - `outcome_score < 0.3` AND `confidence < 0.2`
  - AND not used for 10 sessions

</memory_guidelines>

<problem_solving_guidelines>

When implementing a solution, you are expected to do the following in order:

1. consider the complexity of the problem and whether it can be simplified. if you can solve it in a simpler way that replicates the expected behavior without using a package or library, do so.

2. if the solution becomes too complex without a package, consider using a library that is well-maintained and widely used in the community. if you do use a package, ensure it is compatible with the project's existing dependencies and does not introduce unnecessary bloat. if the package is not well-maintained or has known issues, consider alternatives or implement the functionality yourself.

3. if the solution has no package available and you are absolutely sure you can implement the solution without any external dependencies, you are expected to implement the solution yourself. this includes writing the necessary code, tests, and documentation to ensure the solution is robust and maintainable.

4. if you are stuck on an issue that doesnt have a package available for use or you cannot find a solution within your reliable knowledge cutoff date that doesn't rely on a widely available package/library or known alternative solutions, you have a mcp server with web search capabilities. use it to search for the problem you are facing, and then distill the results down to the most relevant information. do not just copy and paste the search results, but instead summarize them in your own words. you also have access to up to date documentation for the project via the context7 mcp server, so use that to inform your understanding of the codebase and how to solve the problem in case your own internal sources or the web search do not provide enough information.

5. when fixing code, always follow the existing coding style and conventions used in the project. this includes naming conventions, indentation, and commenting style. do not introduce new styles or conventions unless absolutely necessary.

</problem_solving_guidelines>

<PROJECT_CONTEXT>

# SwingMusic Rust - AI Coding Agent Instructions

## Project Overview

SwingMusic Rust is a complete 1:1 rewrite of the Python-based SwingMusic (self-hosted music player) in Rust. The goal is to maintain API compatibility while dramatically improving performance. The project is actively being migrated - the `swingmusic/` subdirectory contains the original Python codebase for reference.

**Tech Stack:** Rust (2021 edition) • Actix-web • SQLx (SQLite) • Tokio • Lofty (audio metadata)

## Architecture Principles

### 1. In-Memory Store Pattern (Critical)

All core data (tracks, albums, artists, folders) loads into **global singleton stores** at startup for sub-millisecond access. This is the performance bottleneck and core architectural decision.

**Store pattern:**

```rust
static TRACK_STORE: OnceLock<Arc<TrackStore>> = OnceLock::new();

pub struct TrackStore {
    tracks: RwLock<HashMap<String, Track>>,
    tracks_by_path: RwLock<HashMap<String, String>>,
    tracks_by_album: RwLock<HashMap<String, Vec<String>>>,
    // Multiple indexes for different lookup patterns
}
```

- **Access:** `TrackStore::get()` returns `Arc<TrackStore>`
- **When to use:** ALL read operations on tracks/albums/artists after startup
- **When to update:** After database writes, reload affected entities into stores
- See: `src/stores/{track_store.rs, album_store.rs, artist_store.rs}`

### 2. Application Startup Sequence

**Critical order** (see `src/main.rs`):

1. `setup_sqlite()` - Initialize SQLite with WAL mode, pragmas
2. `run_migrations()` - Apply schema migrations
3. `maybe_run_initial_scan()` - One-time scan if DB empty
4. `load_into_memory()` - Populate all stores from DB
5. `start_background_tasks()` - Spawn cron jobs (cleanup, periodic scans)
6. Start Actix-web HTTP server

**Never skip steps.** Stores depend on DB being populated; API handlers depend on stores being loaded.

### 3. Hashing System (Entity Identity)

All entities use **xxHash-based deterministic hashes** as primary identifiers:

- **trackhash:** `create_hash([artists, album, title], decode=true)` - 11 chars
- **albumhash:** `create_hash([album, albumartists], decode=true)`
- **artisthash:** `create_hash([name], decode=true)`
- **folderhash:** `create_hash([path], decode=false)`

Hash function: `xxh3_64` → lowercase, strip non-alnum, deunicode → first 11 hex chars

**Why:** Enables deterministic identity without database lookups during indexing. Critical for deduplication and relationships.

See: `src/utils/hashing.rs`

### 4. Metadata Normalization

SwingMusic heavily normalizes metadata during indexing (`src/core/indexer.rs`):

- **Artist splitting:** Configurable separators (`&`, `,`, `feat.`) - respects `artist_split_ignore_list` (e.g., "AC/DC")
- **Featured artists extraction:** Moves `(feat. X)` from titles to artists list
- **Producer removal:** Strips `(prod. by X)` from titles
- **Remaster info:** Removes/extracts `(Remastered)` text
- **Album versioning:** Normalizes "Deluxe Edition" → base album + version label

All controlled by `UserConfig` (`src/config/user_config.rs`). Regex patterns in `src/utils/parsers.rs`.

### 5. Path Handling (Cross-Platform)

**Always normalize paths:**

```rust
use crate::utils::filesystem::normalize_path;
track.filepath = normalize_path(&track.filepath); // Uses forward slashes
```

SQLite stores normalized paths. Critical for Windows/Linux compatibility and consistent store lookups.

## Code Organization

### Module Structure

- **`src/api/`** - Actix-web route handlers (thin layer, delegates to stores/core)
- **`src/core/`** - Business logic libraries (indexer, search, album processing)
- **`src/stores/`** - In-memory data stores (singleton pattern)
- **`src/db/`** - SQLx queries and table interfaces
- **`src/models/`** - Data structures (Track, Album, Artist, etc.)
- **`src/utils/`** - Pure functions (hashing, parsing, filesystem)
- **`src/config/`** - Configuration management (paths, user settings)
- **`src/plugins/`** - External integrations (Last.fm, lyrics)

### API Route Structure

Routes follow Python version's structure for compatibility. Each module defines:

```rust
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/path").route(web::get().to(handler)));
}
```

Upstream compatibility routes use alternate prefixes (`/nothome`, `/notsettings`) for different API clients.

## Common Patterns

### Reading Data (Fast Path)

```rust
use crate::stores::TrackStore;

let store = TrackStore::get();
let track = store.get_by_hash(&trackhash)?;
```

### Writing Data (Update DB + Store)

```rust
use crate::db::tables::TrackTable;

TrackTable::update(&track).await?;
TrackStore::get().update_track(track); // Refresh in-memory copy
```

### Streaming Audio

See `src/api/stream.rs` for HTTP range request handling and on-the-fly transcoding via FFmpeg (actix-web `NamedFile` for ranges).

### Database Queries

Use `DbEngine::get()?.pool()` to access SQLx pool:

```rust
let db = DbEngine::get()?;
sqlx::query_as!(Track, "SELECT * FROM tracks WHERE trackhash = ?", hash)
    .fetch_one(db.pool())
    .await?
```

## Development Workflow

### Build & Run

```powershell
cargo build --release
.\target\release\swingmusic.exe --port 1970
```

Default admin credentials: `admin`/`admin` (change after first login)

### Testing Against Python Version

- Python version runs on port 1970 by default
- API responses should match JSON structure exactly
- Use `swingmusic/` as reference for unclear behavior

### Common Gotchas

1. **Forgot to reload stores after DB write** → Stale data served to clients
2. **Didn't normalize path** → Store lookup fails on Windows
3. **Modified hash logic** → Breaks existing databases (hashes are persistent IDs)
4. **Skipped startup sequence** → Panics on store access before initialization
5. **Missing `deunicode` in hash** → Duplicate entities for accented characters

## Key Files for Reference

- **Startup flow:** `src/main.rs` (lines 60-280)
- **Store pattern:** `src/stores/track_store.rs`
- **Hash generation:** `src/utils/hashing.rs`
- **Metadata parsing:** `src/utils/parsers.rs`
- **Indexing logic:** `src/core/indexer.rs`
- **API structure:** `src/api/mod.rs`
- **Python reference:** `swingmusic/contributing/README.md` (architecture doc)

## Migration Status

This is an **active port** - not all features implemented. Check for `TODO` comments and compare against Python version in `swingmusic/` when implementing new features. Maintain backwards compatibility with existing SwingMusic databases and API clients.


</PROJECT_CONTEXT>
