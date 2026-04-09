import java.util.Map;
import java.util.HashMap;

public class UserService {
    private final Map<String, String> users = new HashMap<>();

    public String getUserEmail(String userId) {
        return users.get(userId);
    }

    public String getGreeting(String userId) {
        String email = getUserEmail(userId);
        return "Hello, " + email.toLowerCase();
    }

    public String getGreetingSafe(String userId) {
        String email = getUserEmail(userId);
        if (email == null) {
            return "Hello, unknown user";
        }
        return "Hello, " + email.toLowerCase();
    }
}
