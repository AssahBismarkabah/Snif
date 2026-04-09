import sqlite3


def find_user(conn: sqlite3.Connection, username: str) -> dict | None:
    """Find user by username."""
    cursor = conn.execute(
        f"SELECT id, email FROM users WHERE name = '{username}'"
    )
    row = cursor.fetchone()
    if row:
        return {"id": row[0], "email": row[1]}
    return None


def find_user_safe(conn: sqlite3.Connection, username: str) -> dict | None:
    """Find user by username using parameterized query."""
    cursor = conn.execute(
        "SELECT id, email FROM users WHERE name = ?",
        (username,)
    )
    row = cursor.fetchone()
    if row:
        return {"id": row[0], "email": row[1]}
    return None
