import sqlite3


def get_order_details(conn: sqlite3.Connection) -> list[dict]:
    """Fetch all orders with customer names."""
    orders = conn.execute("SELECT id, customer_id, total FROM orders").fetchall()

    results = []
    for order in orders:
        customer = conn.execute(
            "SELECT name FROM customers WHERE id = ?", (order[1],)
        ).fetchone()
        results.append({
            "order_id": order[0],
            "customer": customer[0] if customer else "Unknown",
            "total": order[2],
        })
    return results


def get_order_details_safe(conn: sqlite3.Connection) -> list[dict]:
    """Fetch all orders with customer names using a JOIN."""
    rows = conn.execute(
        "SELECT o.id, c.name, o.total "
        "FROM orders o LEFT JOIN customers c ON o.customer_id = c.id"
    ).fetchall()
    return [
        {"order_id": r[0], "customer": r[1] or "Unknown", "total": r[2]}
        for r in rows
    ]
