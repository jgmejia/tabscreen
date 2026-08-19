package ai.tabscreen.client

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.util.Log
import android.view.Surface

private const val TAG = "tabscreen-video"
private const val MIME_TYPE = "video/avc"

// Buffer-sizing hint only, not a hard limit: MediaCodec resolves the real
// frame dimensions from the stream's SPS when decoding straight to a
// Surface. 1920x1080 rather than something larger/exotic deliberately --
// confirmed against the target HONOR Pad X8a's low-latency AVC decoder
// that clamping width/height to their *independent* reported maximums
// (1920 each) produces an invalid 1920x1920 square pairing the codec
// rejects outright (CodecException errorCode=1100); 1920x1080 is about as
// close to a universally-supported joint pairing as exists on AVC
// hardware decoders, sidestepping the need to query joint width/height
// capability ranges for a value that's just a sizing hint anyway.
private const val HINT_WIDTH = 1920
private const val HINT_HEIGHT = 1080

/**
 * Wraps MediaCodec's async decode-to-Surface path for the wire protocol's
 * VIDEO channel (docs/wire-protocol.md): one Annex-B NAL unit per [feed]
 * call, start code included.
 */
class VideoDecoder(surface: Surface) {
    private var codec: MediaCodec? = null
    @Volatile private var running = false
    private var outputThread: Thread? = null

    // logcat is confirmed unreliable on this device/OS (Phase 0) -- these
    // are surfaced through MainActivity's on-screen status instead.
    @Volatile var lastError: String? = null
        private set
    @Volatile var errorCount = 0
        private set
    @Volatile var outputBuffersReleased = 0
        private set

    @Volatile var attempted: String = "not attempted yet"
        private set

    init {
        val info = selectDecoder()
        val caps = info?.getCapabilitiesForType(MIME_TYPE)?.videoCapabilities
        val width = caps?.supportedWidths?.upper?.coerceAtMost(HINT_WIDTH) ?: HINT_WIDTH
        val height = caps?.supportedHeights?.upper?.coerceAtMost(HINT_HEIGHT) ?: HINT_HEIGHT
        attempted = "${info?.name ?: "(platform default)"} at ${width}x$height"
        Log.i(TAG, "using decoder $attempted")

        try {
            val format = MediaFormat.createVideoFormat(MIME_TYPE, width, height)
            val c = if (info != null) MediaCodec.createByCodecName(info.name) else MediaCodec.createDecoderByType(MIME_TYPE)
            c.configure(format, surface, null, 0)
            c.start()
            codec = c
            running = true
            // Draining output on the same thread/cadence as feed() polled
            // with a 0 timeout right after queuing input, which almost
            // never has a decoded buffer ready yet -- confirmed against
            // the target tablet's low-latency decoder (minimal internal
            // pipeline depth by design) that leaving output undrained for
            // long eventually faults the codec entirely (every later call
            // throws IllegalStateException), not just delays rendering.
            // A dedicated thread blocking on dequeueOutputBuffer decouples
            // draining from however fast/slow input arrives.
            outputThread = Thread { drainOutputLoop(c) }.also { it.start() }
        } catch (e: Exception) {
            lastError = "init($attempted): ${describe(e)}"
            Log.e(TAG, "failed to start decoder", e)
        }
    }

    private fun drainOutputLoop(c: MediaCodec) {
        val info = MediaCodec.BufferInfo()
        while (running) {
            try {
                val outIndex = c.dequeueOutputBuffer(info, 10_000)
                if (outIndex >= 0) {
                    c.releaseOutputBuffer(outIndex, true)
                    outputBuffersReleased++
                }
            } catch (e: Exception) {
                errorCount++
                lastError = "output: ${describe(e)}"
                Log.e(TAG, "output drain error", e)
                return
            }
        }
    }

    private fun describe(e: Exception): String {
        val base = "${e.javaClass.simpleName}: ${e.message}"
        return if (e is MediaCodec.CodecException) {
            "$base [errorCode=${e.errorCode} transient=${e.isTransient} recoverable=${e.isRecoverable} diag=${e.diagnosticInfo}]"
        } else {
            base
        }
    }

    /**
     * Confirmed in Phase 0: the target tablet has a dedicated low-latency
     * AVC hardware decoder variant (`c2.qti.avc.decoder.low_latency`) --
     * prefer it explicitly rather than whichever decoder the platform
     * would pick by default.
     */
    private fun selectDecoder(): MediaCodecInfo? {
        val candidates = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { info ->
            !info.isEncoder && info.supportedTypes.any { it.equals(MIME_TYPE, ignoreCase = true) }
        }
        return candidates.find { it.name.contains("low_latency", ignoreCase = true) }
            ?: candidates.firstOrNull { it.isHardwareAccelerated }
            ?: candidates.firstOrNull()
    }

    fun feed(nal: ByteArray) {
        val c = codec ?: return
        try {
            val index = c.dequeueInputBuffer(10_000)
            if (index >= 0) {
                val buffer = c.getInputBuffer(index)
                buffer?.clear()
                buffer?.put(nal)
                c.queueInputBuffer(index, 0, nal.size, System.nanoTime() / 1000, 0)
            }
        } catch (e: Exception) {
            errorCount++
            lastError = "feed: ${describe(e)}"
            Log.e(TAG, "decode error", e)
        }
    }

    fun release() {
        running = false
        outputThread?.join(1000)
        outputThread = null
        try {
            codec?.stop()
            codec?.release()
        } catch (e: Exception) {
            Log.e(TAG, "error releasing decoder", e)
        }
        codec = null
    }
}
