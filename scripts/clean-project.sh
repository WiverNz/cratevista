#!/usr/bin/env bash
set -euo pipefail

apply=0
if [[ $# -gt 1 ]]; then
  echo "Usage: $0 [--apply]" >&2
  exit 2
fi
if [[ $# -eq 1 ]]; then
  if [[ "$1" == "--apply" ]]; then
    apply=1
  else
    echo "Usage: $0 [--apply]" >&2
    exit 2
  fi
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
root="$(cd -- "$script_dir/.." && pwd -P)"
allowlist="$script_dir/clean-project-paths.txt"

format_bytes() {
  local bytes="$1"
  awk -v bytes="$bytes" 'BEGIN {
    if (bytes >= 1073741824) printf "%.2f GiB", bytes / 1073741824;
    else if (bytes >= 1048576) printf "%.2f MiB", bytes / 1048576;
    else if (bytes >= 1024) printf "%.2f KiB", bytes / 1024;
    else printf "%d B", bytes;
  }'
}

display_path() {
  local path="$1"
  local rel="${path#"$root"/}"
  printf '%s\n' "$rel"
}

reject() {
  echo "$1" >&2
  exit 1
}

resolve_entry() {
  local entry="$1"
  local line_no="$2"

  [[ "$entry" == "${entry#"${entry%%[![:space:]]*}"}" && "$entry" == "${entry%"${entry##*[![:space:]]}"}" ]] ||
    reject "Malformed allowlist entry at line $line_no: leading or trailing whitespace is not allowed."
  [[ -n "$entry" ]] ||
    reject "Malformed allowlist entry at line $line_no: empty entries are not allowed."
  [[ "$entry" != /* && "$entry" != \\* && ! "$entry" =~ ^[A-Za-z]:[/\\] ]] ||
    reject "Malformed allowlist entry at line $line_no: paths must be repository-relative."
  [[ "$entry" != *"*"* && "$entry" != *"?"* ]] ||
    reject "Malformed allowlist entry at line $line_no: wildcards are not allowed."

  local normalized_entry="$entry"
  while [[ "$normalized_entry" == */ || "$normalized_entry" == *\\ ]]; do
    normalized_entry="${normalized_entry%/}"
    normalized_entry="${normalized_entry%\\}"
  done
  [[ -n "$normalized_entry" ]] ||
    reject "Malformed allowlist entry at line $line_no: empty entries are not allowed."

  IFS=$'/\\' read -r -a parts <<< "$normalized_entry"
  for part in "${parts[@]}"; do
    [[ -n "$part" && "$part" != "." && "$part" != ".." ]] ||
      reject "Malformed allowlist entry at line $line_no: empty, '.', and '..' path segments are not allowed."
  done

  local target="$root/$normalized_entry"
  local dir base resolved_dir full
  dir="$(dirname -- "$target")"
  base="$(basename -- "$target")"
  if [[ -e "$dir" ]]; then
    resolved_dir="$(cd -- "$dir" && pwd -P)"
    full="$resolved_dir/$base"
  else
    full="$(cd -- "$root" && pwd -P)/$normalized_entry"
  fi

  [[ "$full" != "$root" ]] ||
    reject "Refusing allowlist entry at line $line_no: repository root cannot be removed."
  [[ "$full" == "$root/"* ]] ||
    reject "Refusing allowlist entry at line $line_no: path escapes the repository."
  printf '%s\n' "$full"
}

realpath_portable() {
  local path="$1"
  if command -v realpath >/dev/null 2>&1; then
    realpath -- "$path"
  else
    local dir base
    dir="$(dirname -- "$path")"
    base="$(basename -- "$path")"
    printf '%s/%s\n' "$(cd -- "$dir" && pwd -P)" "$base"
  fi
}

assert_no_escaping_links() {
  local target="$1"
  local link resolved

  if [[ -L "$target" ]]; then
    resolved="$(realpath_portable "$target")"
    [[ "$resolved" == "$root" || "$resolved" == "$root/"* ]] ||
      reject "Refusing to remove $(display_path "$target"): symlink resolves outside the repository to $resolved."
  fi

  if [[ -d "$target" && ! -L "$target" ]]; then
    while IFS= read -r -d '' link; do
      resolved="$(realpath_portable "$link")"
      [[ "$resolved" == "$root" || "$resolved" == "$root/"* ]] ||
        reject "Refusing to remove $(display_path "$link"): symlink resolves outside the repository to $resolved."
    done < <(find -P "$target" -type l -print0)
  fi
}

measure_path_size() {
  local target="$1"
  if [[ -f "$target" && ! -L "$target" ]]; then
    if stat -c '%s' "$target" >/dev/null 2>&1; then
      stat -c '%s' "$target"
    else
      stat -f '%z' "$target"
    fi
  elif [[ -d "$target" && ! -L "$target" ]]; then
    find -P "$target" -type f -exec ls -ln {} + |
      awk '{ total += $5 } END { printf "%d", total + 0 }'
  else
    printf '0'
  fi
}

[[ -f "$allowlist" ]] || reject "Missing cleanup allowlist: $allowlist"

targets=()
line_no=0
while IFS= read -r raw_line || [[ -n "$raw_line" ]]; do
  line_no=$((line_no + 1))
  [[ "$raw_line" == \#* ]] && continue
  target="$(resolve_entry "$raw_line" "$line_no")"
  duplicate=0
  for seen in "${targets[@]}"; do
    if [[ "$seen" == "$target" ]]; then
      duplicate=1
      break
    fi
  done
  [[ "$duplicate" -eq 1 ]] || targets+=("$target")
done < "$allowlist"

[[ "${#targets[@]}" -gt 0 ]] || reject "Cleanup allowlist contains no entries."

existing=()
total=0
for target in "${targets[@]}"; do
  [[ -e "$target" || -L "$target" ]] || continue
  assert_no_escaping_links "$target"
  existing+=("$target")
  size="$(measure_path_size "$target")"
  total=$((total + size))
done

if [[ "$apply" -eq 1 ]]; then
  mode="apply"
else
  mode="dry-run"
fi

echo "CrateVista cleanup ($mode)"
if [[ "${#existing[@]}" -eq 0 ]]; then
  echo "No allowlisted cleanup paths exist."
else
  echo "Paths:"
  for path in "${existing[@]}"; do
    printf '  %s\n' "$(display_path "$path")"
  done
fi
printf 'Total removable size: %s\n' "$(format_bytes "$total")"

if [[ "$apply" -eq 1 ]]; then
  for path in "${existing[@]}"; do
    rm -rf -- "$path"
  done
  echo "Removed ${#existing[@]} allowlisted path(s)."
else
  echo "Dry run only. Re-run with --apply to delete these paths."
fi

echo
echo "git status --short:"
git -C "$root" status --short
