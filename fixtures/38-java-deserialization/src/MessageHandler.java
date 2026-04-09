import java.io.ByteArrayInputStream;
import java.io.ObjectInputStream;
import java.io.IOException;
import java.util.Base64;

public class MessageHandler {
    public Object deserializeMessage(String base64Data) throws IOException, ClassNotFoundException {
        byte[] data = Base64.getDecoder().decode(base64Data);
        ObjectInputStream ois = new ObjectInputStream(new ByteArrayInputStream(data));
        return ois.readObject();
    }

    public String deserializeMessageSafe(String base64Data) {
        byte[] data = Base64.getDecoder().decode(base64Data);
        return new String(data);
    }
}
