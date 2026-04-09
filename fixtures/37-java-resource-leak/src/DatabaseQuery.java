import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.Statement;
import java.sql.SQLException;

public class DatabaseQuery {
    private static final String DB_URL = "jdbc:sqlite:app.db";

    public String findUser(String name) throws SQLException {
        Connection conn = DriverManager.getConnection(DB_URL);
        Statement stmt = conn.createStatement();
        ResultSet rs = stmt.executeQuery(
            "SELECT email FROM users WHERE name = '" + name + "'"
        );
        if (rs.next()) {
            return rs.getString("email");
        }
        return null;
    }

    public String findUserSafe(String name) throws SQLException {
        try (Connection conn = DriverManager.getConnection(DB_URL);
             Statement stmt = conn.createStatement();
             ResultSet rs = stmt.executeQuery(
                 "SELECT email FROM users WHERE name = '" + name + "'"
             )) {
            if (rs.next()) {
                return rs.getString("email");
            }
            return null;
        }
    }
}
