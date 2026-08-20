package ai.tabscreen.client.protocol

// Wire protocol codec — see docs/wire-protocol.md for the spec this
// implements. Keep this in sync with the Rust host's equivalent
// (host/src/protocol.rs).

const val MAX_FRAME_LEN: Long = 4L * 1024 * 1024
private const val HEADER_LEN = 5

enum class Channel(val byte: Int) {
    CONTROL(0),
    VIDEO(1),
    INPUT(2);

    companion object {
        fun fromByte(b: Int): Channel? = values().find { it.byte == b }
    }
}

data class Frame(val channelByte: Int, val channel: Channel?, val payload: ByteArray)

fun encodeFrame(channel: Channel, payload: ByteArray): ByteArray {
    require(payload.size.toLong() <= MAX_FRAME_LEN) { "payload of ${payload.size} bytes exceeds MAX_FRAME_LEN" }
    val out = ByteArray(HEADER_LEN + payload.size)
    val len = payload.size
    out[0] = (len and 0xFF).toByte()
    out[1] = ((len shr 8) and 0xFF).toByte()
    out[2] = ((len shr 16) and 0xFF).toByte()
    out[3] = ((len shr 24) and 0xFF).toByte()
    out[4] = channel.byte.toByte()
    payload.copyInto(out, HEADER_LEN)
    return out
}

/**
 * Incremental parser: bulk reads don't align with frame boundaries (a
 * single read may contain a partial frame, multiple frames, or a frame
 * spanning several reads), so bytes are fed in as they arrive and frames
 * are pulled out as they become complete. Mirrors host/src/protocol.rs's
 * FrameReader.
 */
class FrameReader {
    private var buf = ByteArray(16384)
    private var size = 0

    fun feed(data: ByteArray, len: Int = data.size) {
        ensureCapacity(size + len)
        System.arraycopy(data, 0, buf, size, len)
        size += len
    }

    /** Returns the next complete frame if fully buffered, or null if more data is needed. */
    fun tryReadFrame(): Frame? {
        if (size < HEADER_LEN) return null

        val length = (buf[0].toInt() and 0xFF) or
            ((buf[1].toInt() and 0xFF) shl 8) or
            ((buf[2].toInt() and 0xFF) shl 16) or
            ((buf[3].toInt() and 0xFF) shl 24)
        if (length < 0 || length.toLong() > MAX_FRAME_LEN) {
            throw IllegalStateException("frame length $length exceeds MAX_FRAME_LEN")
        }

        val totalLen = HEADER_LEN + length
        if (size < totalLen) return null

        val channelByte = buf[4].toInt() and 0xFF
        val payload = buf.copyOfRange(HEADER_LEN, totalLen)

        val remaining = size - totalLen
        System.arraycopy(buf, totalLen, buf, 0, remaining)
        size = remaining

        return Frame(channelByte, Channel.fromByte(channelByte), payload)
    }

    private fun ensureCapacity(needed: Int) {
        if (buf.size < needed) {
            val newCap = maxOf(needed, buf.size * 2)
            buf = buf.copyOf(newCap)
        }
    }
}

object Control {
    const val HELLO: Int = 0x01
    const val HELLO_ACK: Int = 0x02
    const val DISPLAY_INFO: Int = 0x03

    fun encodeHello(protocolVersion: Int): ByteArray = byteArrayOf(HELLO.toByte(), protocolVersion.toByte())
    fun encodeHelloAck(protocolVersion: Int): ByteArray = byteArrayOf(HELLO_ACK.toByte(), protocolVersion.toByte())

    /**
     * Reports this device's *current effective* display mode (Phase 5,
     * docs/wire-protocol.md) -- not a fixed hardware constant. `width`/
     * `height` are in the device's current rotation; `refreshHz` is
     * rounded to the nearest integer. Sent once, immediately after
     * HELLO_ACK -- the client is orientation-locked (AndroidManifest.xml),
     * so there's no runtime change to report again later.
     */
    fun encodeDisplayInfo(width: Int, height: Int, refreshHz: Int): ByteArray {
        val out = ByteArray(6)
        out[0] = DISPLAY_INFO.toByte()
        out[1] = (width and 0xFF).toByte()
        out[2] = ((width shr 8) and 0xFF).toByte()
        out[3] = (height and 0xFF).toByte()
        out[4] = ((height shr 8) and 0xFF).toByte()
        out[5] = refreshHz.toByte()
        return out
    }
}

enum class TouchAction(val byte: Int) {
    DOWN(0), MOVE(1), UP(2), CANCEL(3);

    companion object {
        fun fromByte(b: Int): TouchAction? = values().find { it.byte == b }
    }
}

data class TouchEvent(val action: TouchAction, val pointerId: Int, val x: Float, val y: Float, val pressure: Float)

/**
 * Client -> host only (docs/wire-protocol.md's INPUT channel). x/y are
 * normalized 0.0-1.0 relative to this device's screen, not raw pixels, so
 * the host can map onto whatever the target output's pixel rect is without
 * the client needing to know the host's resolution.
 */
object Input {
    const val TOUCH: Int = 0x01
    private const val TOUCH_LEN = 15

    fun encodeTouch(ev: TouchEvent): ByteArray {
        val out = ByteArray(TOUCH_LEN)
        out[0] = TOUCH.toByte()
        out[1] = ev.action.byte.toByte()
        out[2] = ev.pointerId.toByte()
        writeFloatLE(out, 3, ev.x)
        writeFloatLE(out, 7, ev.y)
        writeFloatLE(out, 11, ev.pressure)
        return out
    }

    fun decodeTouch(payload: ByteArray): TouchEvent {
        require(payload.size >= TOUCH_LEN && (payload[0].toInt() and 0xFF) == TOUCH) {
            "malformed TOUCH payload"
        }
        val action = TouchAction.fromByte(payload[1].toInt() and 0xFF)
            ?: throw IllegalArgumentException("unknown touch action byte ${payload[1]}")
        return TouchEvent(
            action = action,
            pointerId = payload[2].toInt() and 0xFF,
            x = readFloatLE(payload, 3),
            y = readFloatLE(payload, 7),
            pressure = readFloatLE(payload, 11),
        )
    }

    private fun writeFloatLE(out: ByteArray, offset: Int, value: Float) {
        val bits = value.toRawBits()
        out[offset] = (bits and 0xFF).toByte()
        out[offset + 1] = ((bits shr 8) and 0xFF).toByte()
        out[offset + 2] = ((bits shr 16) and 0xFF).toByte()
        out[offset + 3] = ((bits shr 24) and 0xFF).toByte()
    }

    private fun readFloatLE(b: ByteArray, offset: Int): Float {
        val bits = (b[offset].toInt() and 0xFF) or
            ((b[offset + 1].toInt() and 0xFF) shl 8) or
            ((b[offset + 2].toInt() and 0xFF) shl 16) or
            ((b[offset + 3].toInt() and 0xFF) shl 24)
        return Float.fromBits(bits)
    }
}
