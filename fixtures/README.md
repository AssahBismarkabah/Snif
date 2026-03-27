# Benchmark Fixtures

This directory contains the evaluation benchmark set for Snif. Each fixture is
a directory with real source files, a unified diff, and metadata describing
expected findings. The `snif eval` command runs every fixture through the full
review pipeline and measures precision, recall, and noise rate against the
expected results.


# Structure

Each fixture is a directory containing:

`fixture.json` holds the fixture name, description, and expected findings. It
does not contain source code or diffs — those live as real files alongside it.

`change.patch` is the unified diff representing the code change being reviewed.

Source files (under `src/` or other paths) are the actual files after the
change. These are real, editable source files that can be syntax-checked
directly.


# Fixture Categories

The benchmark set is composed of three categories.

Bug fixtures contain deliberate, verifiable issues that the reviewer should
catch. Each has one or more entries in `expected_findings` pointing to the file
and line where the bug exists. Categories include logic errors, security
vulnerabilities, and convention violations.

Clean fixtures contain well-written code with no issues. The reviewer should
return an empty array. These verify that the tool stays quiet on clean changes.

Style fixtures contain code with formatting or style inconsistencies but no
logic bugs or security issues. The reviewer should return an empty array.
These verify that style-only noise is suppressed.


# Running

```
snif eval --fixtures ./fixtures/
```

Quality gates pass if precision is at least 70% and noise rate is at most 20%.
The aspirational targets are precision at least 80%, recall at least 60%, and
noise rate under 10%.


# Adding Fixtures

Create a new directory under `fixtures/` with:

1. Source files in their real directory structure
2. A `change.patch` with the unified diff
3. A `fixture.json` with metadata:

```json
{
  "name": "descriptive-name",
  "description": "What this fixture tests",
  "conventions": null,
  "expected_findings": [
    {
      "file": "src/example.rs",
      "start_line": 12,
      "category": "logic"
    }
  ]
}
```

For clean and style fixtures, set `expected_findings` to an empty array.

Before committing a new fixture, verify the source code is accurate. Every
"clean" fixture must be genuinely clean — no hidden edge cases, no subtle bugs,
no compiler warnings. The eval harness catches fixture inaccuracies because
the model finds real issues the fixture author missed.
