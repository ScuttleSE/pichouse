// Package version holds the application version, read and updated by CI.
//
// Versioning policy (see AGENTS.md, RULE THREE):
//   - Format is major.minor.build.
//   - The series starts at 0.0.0.
//   - CI increments build by 1 on every push to main.
//   - A release bumps major, minor, or build on request.
package version

// Version is the current application version (major.minor.build).
const Version = "0.0.5"
