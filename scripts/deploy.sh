#!/usr/bin/env bash
# Run on the host that owns the Compose stack, after CI has finished.
set -euo pipefail

usage() {
    printf 'Usage: %s <merge-branch> [--check]\n' "$0" >&2
}

fail() {
    printf 'Deploy aborted: %s\n' "$1" >&2
    exit 1
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    usage
    exit 2
fi
branch=$1
check_only=false
if [ "$#" -eq 2 ]; then
    if [ "$2" != '--check' ]; then
        usage
        exit 2
    fi
    check_only=true
fi

rollback_tag=
status_file=
logs_file=
restore_main=false
old_commit=
cleanup() {
    result=$?
    if [ "$result" -ne 0 ] && [ "$restore_main" = true ]; then
        git reset --keep "$old_commit" >/dev/null || printf 'Could not restore local main\n' >&2
    fi
    [ -z "$status_file" ] || rm -f "$status_file" || true
    [ -z "$logs_file" ] || rm -f "$logs_file" || true
    if [ "$result" -ne 0 ] && [ -n "$rollback_tag" ]; then
        printf 'Rollback image tag: %s\n' "$rollback_tag" >&2
    fi
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

check_preconditions() {
    local root local_sha remote_sha main_sha checkout_state
    root=$(git rev-parse --show-toplevel 2>/dev/null) || fail 'not in a Git repository'
    [ "$(pwd -P)" = "$(cd "$root" && pwd -P)" ] || fail 'run from the repository root'
    [ "$(git branch --show-current)" = main ] || fail 'checkout must be on main'
    checkout_state=$(git status --porcelain --untracked-files=all) || fail 'could not inspect checkout'
    [ -z "$checkout_state" ] || fail 'checkout must be clean'
    git check-ref-format --branch "$branch" >/dev/null 2>&1 || fail 'invalid merge branch name'
    [ "$branch" != main ] || fail 'merge branch must differ from main'
    local_sha=$(git rev-parse --verify "refs/heads/$branch" 2>/dev/null) || fail 'local merge branch is missing'
    remote_sha=$(git ls-remote --exit-code origin "refs/heads/$branch" 2>/dev/null) || fail 'origin merge branch is missing or unavailable'
    remote_sha=${remote_sha%%[[:space:]]*}
    [ "$local_sha" = "$remote_sha" ] || fail 'local merge branch differs from origin'
    main_sha=$(git rev-parse HEAD)
    git merge-base --is-ancestor "$main_sha" "$local_sha" || fail 'main is not an ancestor of the merge branch'
    merge_sha=$local_sha
}

print_ci_urls() {
    printf '%s\n' "$1" | sed -nE 's@.*(https://[^[:space:]]+/actions/runs/[0-9]+).*@CI run: \1@p' >&2
}

check_ci() {
    local run status conclusion url reported_sha reported_branch canonical
    run=$(gh run list --workflow ci.yml --branch "$branch" --commit "$merge_sha" --event push \
        --limit 100 --json status,conclusion,url,headSha,headBranch,createdAt \
        --jq 'sort_by(.createdAt) | reverse | .[0] | if . == null then empty else [.status, (if .conclusion == null or .conclusion == "" then "none" else .conclusion end), .url, .headSha, .headBranch] | @tsv end') || {
        print_ci_urls "$run"
        fail 'could not query the CI workflow'
    }
    [ -n "$run" ] || fail 'no CI workflow run found for the merge branch SHA'
    print_ci_urls "$run"
    IFS="$(printf '\t')" read -r status conclusion url reported_sha reported_branch <<< "$run"
    canonical=$(printf '%s\t%s\t%s\t%s\t%s' "$status" "$conclusion" "$url" "$reported_sha" "$reported_branch")
    [ "$run" = "$canonical" ] || fail 'malformed CI workflow response'
    case "$url" in
        https://*/actions/runs/[0-9]*) ;;
        *) fail 'CI workflow response has no run URL' ;;
    esac
    [ "$reported_sha" = "$merge_sha" ] && [ "$reported_branch" = "$branch" ] \
        || fail "CI run does not match the merge branch SHA: $url"
    [ "$status" = completed ] && [ "$conclusion" = success ] \
        || fail "CI run is $status/$conclusion: $url"
    printf 'CI passed for %s: %s\n' "$merge_sha" "$url"
}

preserve_running_image() {
    local container image digest
    container=$(docker compose ps -q gateway) || fail 'could not find the running gateway'
    [ -n "$container" ] || fail 'no running gateway to preserve'
    image=$(docker container inspect --format '{{.Image}}' "$container") || fail 'could not inspect the running image'
    digest=${image#sha256:}
    [ "$image" != "$digest" ] && [ "${#digest}" -eq 64 ] || fail 'invalid running image ID'
    case "$digest" in
        *[!0-9a-f]*) fail 'invalid running image ID' ;;
    esac
    # The mutable build tag may already point at an unsuccessful deployment.
    docker image tag "$image" "pinchora:rollback-$digest" || fail 'could not preserve the previous image'
    rollback_tag="pinchora:rollback-$digest"
}

advance_main() {
    old_commit=$(git rev-parse HEAD) || fail 'could not resolve main'
    restore_main=true
    git merge -q --ff-only "$merge_sha" || fail 'fast-forward merge failed'
    git push -q origin main || fail 'push of main failed'
    restore_main=false
}

backup_database() {
    local backup_dir backup_name backup_file
    backup_dir=${PINCHORA_BACKUP_DIR:-$HOME/Development/pinchora-backups}
    mkdir -p "$backup_dir" 2>/dev/null || fail 'could not create backup directory'
    backup_name=${branch//\//-}
    backup_file="$backup_dir/pinchora-before-$backup_name-$(date +%Y%m%d-%H%M).dump"
    [ ! -e "$backup_file" ] || fail 'backup destination already exists'
    # Open the destination separately so shell errors cannot disclose the override path.
    if ! { exec 3> "$backup_file"; } 2>/dev/null; then
        fail 'could not open backup destination'
    fi
    if ! docker compose exec -T postgres pg_dump -U pinchora -d pinchora -Fc >&3; then
        exec 3>&-
        rm -f "$backup_file" 2>/dev/null || true
        fail 'database dump failed'
    fi
    exec 3>&-
    if [ ! -s "$backup_file" ]; then
        rm -f "$backup_file" 2>/dev/null || true
        fail 'database dump is empty'
    fi
    printf 'Database backup saved.\n'
}

expected_migrations() {
    local file version number
    migration_count=0
    migration_max=0
    for file in migrations/[0-9]*.sql; do
        [ -f "$file" ] || continue
        version=${file##*/}
        version=${version%%_*}
        number=$((10#$version))
        migration_count=$((migration_count + 1))
        if [ "$number" -gt "$migration_max" ]; then
            migration_max=$number
        fi
    done
    [ "$migration_count" -gt 0 ] || fail 'no migration files found'
}

http_status() {
    curl --silent --show-error --connect-timeout 5 --max-time 15 \
        --output "$2" --write-out '%{http_code}' \
        "http://127.0.0.1:8090$1"
}

verify_deployment() {
    local code route actual expected errors failed
    failed=0
    status_file=$(mktemp) || fail 'could not create status file'
    logs_file=$(mktemp) || fail 'could not create log file'

    code=$(http_status / /dev/null) || code=000
    printf 'GET /: %s\n' "$code"
    [ "$code" = 200 ] || failed=1

    code=$(http_status /api/public/status "$status_file") || code=000
    printf 'GET /api/public/status: %s\n' "$code"
    if [ "$code" != 200 ] || ! grep -Eq '"version"[[:space:]]*:[[:space:]]*"'"$new_sha"'[[:space:]]' "$status_file"; then
        printf 'Public status does not report the deployed SHA\n' >&2
        failed=1
    fi

    expected_migrations
    expected="$migration_max|$migration_count"
    actual=$(docker compose exec -T postgres psql -U pinchora -d pinchora -At -F '|' \
        -c 'select coalesce(max(version), 0), count(*) from _sqlx_migrations') || actual=unavailable
    printf 'Migrations: %s (expected %s)\n' "$actual" "$expected"
    [ "$actual" = "$expected" ] || failed=1

    if docker compose logs --since 2m gateway web > "$logs_file"; then
        errors=$(grep -c ERROR "$logs_file") || true
        printf 'Recent ERROR lines: %s\n' "$errors"
        [ "$errors" -eq 0 ] || failed=1
    else
        printf 'Could not read recent gateway and web logs\n' >&2
        failed=1
    fi

    for route in /me /commands /me/wallet /me/collection /me/house; do
        code=$(http_status "$route" /dev/null) || code=000
        printf 'GET %s: %s\n' "$route" "$code"
        case "$code" in
            2??|3??) ;;
            *) failed=1 ;;
        esac
    done
    [ "$failed" -eq 0 ] || fail 'post-deploy verification failed'
}

check_preconditions
check_ci
if [ "$check_only" = true ]; then
    printf 'All deploy checks passed.\n'
    exit 0
fi

backup_database
preserve_running_image
advance_main
new_sha=$(git rev-parse --short HEAD)
export GIT_SHA="$new_sha"
docker compose build gateway || fail 'gateway image build failed'
docker image tag pinchora:dev "pinchora:$new_sha" || fail 'could not tag the deployed image'
docker compose up -d || fail 'could not bring up the stack'
docker compose run --rm gateway register || fail 'command registration failed'
docker compose restart gateway web || fail 'gateway/web restart failed'
sleep 25
verify_deployment
printf 'Deployment verified: pinchora:%s\n' "$new_sha"
