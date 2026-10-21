package com.example;

public enum TinyEnum {
    First(0),
    Second(7);

    private final int value;

    TinyEnum(int value) {
        this.value = value;
    }

    public int getValue() {
        return value;
    }
}
