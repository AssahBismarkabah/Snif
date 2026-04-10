import java.io.ByteArrayInputStream;
import java.io.ObjectInputStream;
import java.io.IOException;

public class MessageHandler {
    public Object deserializeMessage(byte[] data) throws IOException, ClassNotFoundException {
        try (ObjectInputStream ois = new ObjectInputStream(new ByteArrayInputStream(data))) {
            return ois.readObject();
        }
    }

    public int rawMessageSize(byte[] data) {
        return data.length;
    }
}
