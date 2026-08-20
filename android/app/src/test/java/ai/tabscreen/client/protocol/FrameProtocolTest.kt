package ai.tabscreen.client.protocol

import org.junit.Assert.*
import org.junit.Test

class FrameProtocolTest {

    @Test
    fun `round trips a single frame`() {
        val encoded = encodeFrame(Channel.CONTROL, Control.encodeHello(1))
        val reader = FrameReader()
        reader.feed(encoded)
        val frame = reader.tryReadFrame()!!
        assertEquals(Channel.CONTROL, frame.channel)
        assertArrayEquals(Control.encodeHello(1), frame.payload)
        assertNull(reader.tryReadFrame())
    }

    @Test
    fun `handles a frame split across multiple feeds`() {
        val payload = ByteArray(1000) { 0xAB.toByte() }
        val encoded = encodeFrame(Channel.VIDEO, payload)
        val reader = FrameReader()
        reader.feed(encoded.copyOfRange(0, 3))
        assertNull(reader.tryReadFrame())
        reader.feed(encoded.copyOfRange(3, 500))
        assertNull(reader.tryReadFrame())
        reader.feed(encoded.copyOfRange(500, encoded.size))
        val frame = reader.tryReadFrame()!!
        assertEquals(Channel.VIDEO, frame.channel)
        assertEquals(1000, frame.payload.size)
    }

    @Test
    fun `handles multiple frames in one feed`() {
        val a = encodeFrame(Channel.CONTROL, byteArrayOf(1, 2, 3))
        val b = encodeFrame(Channel.INPUT, byteArrayOf(4, 5))
        val reader = FrameReader()
        reader.feed(a + b)
        val f1 = reader.tryReadFrame()!!
        val f2 = reader.tryReadFrame()!!
        assertArrayEquals(byteArrayOf(1, 2, 3), f1.payload)
        assertArrayEquals(byteArrayOf(4, 5), f2.payload)
    }

    @Test(expected = IllegalStateException::class)
    fun `rejects a frame length exceeding the cap`() {
        val reader = FrameReader()
        val bogusLen = (MAX_FRAME_LEN + 1).toInt()
        val header = byteArrayOf(
            (bogusLen and 0xFF).toByte(),
            ((bogusLen shr 8) and 0xFF).toByte(),
            ((bogusLen shr 16) and 0xFF).toByte(),
            ((bogusLen shr 24) and 0xFF).toByte(),
            0
        )
        reader.feed(header)
        reader.tryReadFrame()
    }

    @Test
    fun `unknown channel byte round trips as unknown`() {
        val reader = FrameReader()
        reader.feed(byteArrayOf(5, 0, 0, 0, 99, 1, 2, 3, 4, 5))
        val frame = reader.tryReadFrame()!!
        assertEquals(99, frame.channelByte)
        assertNull(frame.channel)
    }

    @Test
    fun `recognizes a hello ack reply`() {
        val encoded = encodeFrame(Channel.CONTROL, Control.encodeHelloAck(1))
        val reader = FrameReader()
        reader.feed(encoded)
        val frame = reader.tryReadFrame()!!
        assertEquals(Control.HELLO_ACK, frame.payload[0].toInt() and 0xFF)
        assertEquals(1, frame.payload[1].toInt() and 0xFF)
    }

    @Test
    fun `encodes display info as little-endian width, height, refresh`() {
        val encoded = Control.encodeDisplayInfo(width = 1920, height = 1200, refreshHz = 60)
        assertEquals(6, encoded.size)
        assertEquals(Control.DISPLAY_INFO, encoded[0].toInt() and 0xFF)
        assertEquals(1920, (encoded[1].toInt() and 0xFF) or ((encoded[2].toInt() and 0xFF) shl 8))
        assertEquals(1200, (encoded[3].toInt() and 0xFF) or ((encoded[4].toInt() and 0xFF) shl 8))
        assertEquals(60, encoded[5].toInt() and 0xFF)
    }

    @Test
    fun `round trips a touch event`() {
        val ev = TouchEvent(TouchAction.MOVE, pointerId = 0, x = 0.25f, y = 0.75f, pressure = 0.5f)
        val encoded = Input.encodeTouch(ev)
        val decoded = Input.decodeTouch(encoded)
        assertEquals(ev, decoded)
    }

    @Test(expected = IllegalArgumentException::class)
    fun `rejects a truncated touch payload`() {
        val encoded = Input.encodeTouch(TouchEvent(TouchAction.DOWN, 0, 0f, 0f, 1f))
        Input.decodeTouch(encoded.copyOfRange(0, encoded.size - 1))
    }

    @Test(expected = IllegalArgumentException::class)
    fun `rejects an unknown touch action byte`() {
        val encoded = Input.encodeTouch(TouchEvent(TouchAction.DOWN, 0, 0f, 0f, 1f))
        encoded[1] = 0xEE.toByte()
        Input.decodeTouch(encoded)
    }
}
