# branchyard

Paired Git Worktrees CLI — create isolated environments across multiple repos simultaneously.

## The problem

Working on multiple features in parallel across several repositories is painful. The common workaround is switching branches with `git stash` or `git checkout`, but this breaks running processes, resets your database state, and forces you to restart servers every time you context-switch.

A single feature in a typical product stack might span a frontend, a backend API, and a few microservices — all of which need to run simultaneously, on different ports, backed by isolated databases. Setting this up manually for each feature means managing port conflicts, Docker container name collisions, and repeated setup steps across every repo. Done is worse: you have to remember every branch, every container, and every worktree you created.

## How branchyard solves it

Branchyard automates the full lifecycle of a feature environment. You run one command and it:

- Creates a **git worktree** in every configured repo on the same feature branch
- Runs per-repo **setup commands** (installs, migrations, seed data)
- Generates a **standalone Docker Compose file** with unique container names and ports, isolated from every other active environment
- Opens your **terminal multiplexer** (Warp, iTerm2, tmux, Ghostty) with one tab per repo, ready to serve

When the feature ships, `branchyard done` tears down everything — worktrees, containers, and local branches — across all repos in one command.

Port assignment is deterministic: each environment gets a **slot** (0, 1, 2…), and ports are calculated from that slot, so multiple environments never conflict.

```
branchyard new auth-jwt        Create paired worktrees for all configured repos
branchyard serve auth-jwt      Start services (docker compose up + dev servers)
branchyard stop auth-jwt       Stop services
branchyard list                List active worktrees with ports
branchyard done auth-jwt       Remove worktree, containers, and branches
branchyard done --all          Remove all active worktrees at once
branchyard init                Configure this workspace (creates .branchyard.yml)
```

## Workflow

```bash
# Once — creates worktrees, runs setup commands, opens terminal
branchyard new auth-jwt

# Start Docker services and dev servers
branchyard serve auth-jwt

# Stop Docker services (dev servers stay in terminal tabs — stop them with Ctrl+C)
branchyard stop auth-jwt

# Start again after a stop
branchyard serve auth-jwt

# When the feature is done — removes worktrees, containers, and local branches
branchyard done auth-jwt
```

`new` runs once per feature. `serve` and `stop` can be used as many times as needed.

---

## Setup

Branchyard is stack-agnostic. It adapts to any project through a single configuration file — `.branchyard.yml` — that lives in your workspace root (the directory that contains all your repos). This file declares your repositories, Docker services, ports, and terminal integration. Branchyard reads it at runtime to know what to create, start, and tear down.

Run `branchyard init` to generate it interactively:

```bash
cd ~/projects/my-workspace   # the directory that contains your repos
branchyard init
```

The wizard asks for repos, serve commands, Docker services, and port base. It creates `.branchyard.yml`.

**Edit the file manually** to add symlinks, one-time setup commands, or hooks. See `example.branchyard.yml` in this repo for a full reference.

---

## Configuration reference (`.branchyard.yml`)

```yaml
base_branch: main
worktrees_dir: ./worktrees

repos:
  - name: frontend
    path: ./my-frontend
    commands:
      serve: "npm run dev -- --port {port}"
      setup: "npm ci"            # runs once on `branchyard new`

  - name: backend
    path: ./my-backend
    commands:
      serve: "bundle exec rails server -p {port}"
      setup: "bundle exec rails db:create db:migrate"

ports:
  base: 3000                     # slot 0 → 3000..., slot 1 → 3010..., etc.

services:                        # Docker services — each gets isolated ports and container names per worktree
  - name: postgres
    image: postgres:16
    port: 5432                   # container port (host port assigned automatically per slot)
    environment:
      - POSTGRES_PASSWORD=postgres

  - name: redis
    image: redis:7
    port: 6379

  - name: web                    # app server — use for services with complex config
    image: my-app:latest
    build: ./my-backend          # build context (relative to workspace root); builds locally instead of pulling
    port: 3000
    command: "bundle exec rails s -p 3000 -b 0.0.0.0"
    env_file: ./my-backend/.env  # resolved relative to workspace root
    volumes:
      - bundle_cache:/usr/local/bundle
    depends_on:
      - postgres
      - redis

terminal:
  multiplexer: warp              # warp | iterm2 | tmux | ghostty | none (default: none)

hooks:
  after_new:
    - "echo 'Worktree {slug} ready'"
  after_done: []
```

### Variable interpolation

Use these placeholders in `commands.serve`, `commands.setup`, and `hooks`:

| Variable | Value |
|---|---|
| `{slug}` | Worktree name (e.g. `auth-jwt`) |
| `{port}` | This repo's assigned port |
| `{workspace}` | Absolute path to the workspace root (where `.branchyard.yml` lives) |
| `{<name>_port}` | Port of any repo or service named `<name>` (e.g. `{backend_port}`, `{postgres_port}`) |

---

## Examples

### Node + Django + Postgres + Redis

```yaml
base_branch: main
worktrees_dir: ./worktrees

repos:
  - name: web
    path: ./next-app
    commands:
      serve: "npm run dev -- --port {port}"
      setup: "npm ci"

  - name: api
    path: ./django-api
    commands:
      serve: "python manage.py runserver {port}"
      setup: "python manage.py migrate"

ports:
  base: 3000

services:
  - name: postgres
    image: postgres:16
    port: 5432
    environment:
      - POSTGRES_PASSWORD=postgres
  - name: redis
    image: redis:7
    port: 6379

terminal:
  multiplexer: warp
```

With `base: 3000` and 2 repos:

| Slot | web  | api  | postgres | redis |
|------|------|------|----------|-------|
| 0    | 3000 | 3001 | 3002     | 3003  |
| 1    | 3010 | 3011 | 3012     | 3013  |
| 2    | 3020 | 3021 | 3022     | 3023  |

---

### Three services (Go API + Next.js + Python worker)

```yaml
base_branch: develop
worktrees_dir: ./worktrees

repos:
  - name: api
    path: ./go-api
    commands:
      serve: "go run ./cmd/server -port {port}"

  - name: web
    path: ./next-web
    commands:
      serve: "npm run dev -- --port {port}"

  - name: worker
    path: ./py-worker
    commands:
      serve: "python worker.py --redis redis://localhost:{redis_port}"

ports:
  base: 8000

services:
  - name: redis
    image: redis:7
    port: 6379
```

---

### With symlinks (avoids reinstalling node_modules per worktree)

```yaml
repos:
  - name: frontend
    path: ./my-frontend
    commands:
      serve: "npm run dev -- --port {port}"
    setup:
      symlinks:
        - from: ./my-frontend/node_modules
          to: node_modules
```

---

## How it works

Each feature gets **one worktree per repo** on the same branch, with an isolated runtime environment:

```
worktrees/
└── auth-jwt/
    ├── frontend/                       ← git worktree (branch: auth-jwt)
    ├── backend/                        ← git worktree (branch: auth-jwt)
    ├── docker-compose.override.yml     ← standalone compose with unique ports and container names
    ├── .slot                           ← port slot number (0, 1, 2...)
    └── .branch                         ← original branch name
```

Port assignment is deterministic from the slot number. Slots are assigned on `branchyard new` and freed on `branchyard done`. Multiple worktrees never conflict on ports or container names.

Each service in `docker-compose.override.yml` gets a unique `container_name` prefixed by slug (e.g. `auth-jwt-postgres`, `auth-jwt-redis`). The compose file is standalone — it does not extend any base `docker-compose.yml`, so parallel worktrees are fully independent.

---

## Shell completion

```bash
# Zsh
branchyard completion zsh > ~/.zfunc/_branchyard

# Bash
branchyard completion bash > /etc/bash_completion.d/branchyard

# Fish
branchyard completion fish > ~/.config/fish/completions/branchyard.fish
```

## License

MIT
