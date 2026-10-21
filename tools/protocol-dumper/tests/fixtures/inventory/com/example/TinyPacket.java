package com.example;

import com.hypixel.hytale.protocol.NetworkChannel;
import com.hypixel.hytale.protocol.Packet;
import com.hypixel.hytale.protocol.ToServerPacket;
import javax.annotation.Nullable;

public class TinyPacket implements Packet, ToServerPacket {
    public static final int PACKET_ID = 42;
    public static final boolean IS_COMPRESSED = true;
    public static final int NULLABLE_BIT_FIELD_SIZE = 1;
    public static final int FIXED_BLOCK_SIZE = 5;
    public static final int VARIABLE_FIELD_COUNT = 1;
    public static final int VARIABLE_BLOCK_START = 9;
    public static final int MAX_SIZE = 64;

    public int value;
    @Nullable public String name;

    @Override
    public NetworkChannel getChannel() {
        return NetworkChannel.Chunks;
    }

    public int computeSize() {
        return 5;
    }
}
