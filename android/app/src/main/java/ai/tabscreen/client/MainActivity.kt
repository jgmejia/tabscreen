package ai.tabscreen.client

import ai.tabscreen.client.protocol.Channel
import ai.tabscreen.client.protocol.Control
import ai.tabscreen.client.protocol.Frame
import ai.tabscreen.client.protocol.FrameReader
import ai.tabscreen.client.protocol.Input
import ai.tabscreen.client.protocol.TouchAction
import ai.tabscreen.client.protocol.TouchEvent
import ai.tabscreen.client.protocol.encodeFrame
import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.util.Log
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.TextView
import java.io.FileInputStream
import java.io.FileOutputStream

private const val TAG = "tabscreen"
private const val ACTION_USB_PERMISSION = "ai.tabscreen.client.USB_PERMISSION"
private const val PROTOCOL_VERSION = 1

/**
 * Phase 1/4 scope: opens the AOA accessory, completes the CONTROL
 * HELLO/HELLO_ACK handshake (docs/wire-protocol.md), captures touch input
 * over the whole window and sends it on the INPUT channel. There's no video
 * surface yet (Phase 3), so touches are captured against the full window
 * rather than a specific SurfaceView for now.
 */
class MainActivity : Activity() {
    private lateinit var statusView: TextView
    private lateinit var usbManager: UsbManager
    private var fd: ParcelFileDescriptor? = null
    private var readerThread: Thread? = null
    private var pollThread: Thread? = null
    @Volatile private var pollingActive = false
    @Volatile private var output: FileOutputStream? = null
    private val outputLock = Any()

    // Confirmed in Phase 0: the accessory-attached intent-filter doesn't
    // reliably bring this app to the foreground on MagicOS, even though
    // the permission grant succeeds silently. This receiver + the
    // accessoryList/requestPermission fallback below cover the case where
    // the app is launched manually instead of auto-launched.
    //
    // Also handles ATTACHED/DETACHED at runtime, not just at launch: a
    // physical unplug/replug while the app is already running needs to be
    // detected and reconnected without relying on the OS relaunching the
    // Activity, since that launch path is already known to be unreliable
    // here.
    private val usbReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            @Suppress("DEPRECATION")
            val accessory: UsbAccessory? = intent.getParcelableExtra(UsbManager.EXTRA_ACCESSORY)
            when (intent.action) {
                ACTION_USB_PERMISSION -> {
                    val granted = intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
                    Log.i(TAG, "permission result granted=$granted accessory=$accessory")
                    if (granted && accessory != null) {
                        openAccessory(accessory)
                    } else {
                        setStatus("Permission denied for accessory")
                    }
                }
                UsbManager.ACTION_USB_ACCESSORY_ATTACHED -> {
                    Log.i(TAG, "runtime ATTACHED broadcast: $accessory")
                    if (accessory != null) requestOrOpen(accessory)
                }
                UsbManager.ACTION_USB_ACCESSORY_DETACHED -> {
                    Log.i(TAG, "runtime DETACHED broadcast: $accessory")
                    closeConnection()
                    setStatus("Disconnected. Waiting for accessory...")
                    startPolling()
                }
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        statusView = TextView(this)
        statusView.textSize = 20f
        statusView.setPadding(32, 64, 32, 32)

        val root = FrameLayout(this)
        root.addView(statusView)
        root.setOnTouchListener { view, event -> handleTouch(view, event) }
        setContentView(root)
        setStatus("Starting...")

        usbManager = getSystemService(USB_SERVICE) as UsbManager

        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU)
            Context.RECEIVER_NOT_EXPORTED else 0
        val filter = IntentFilter().apply {
            addAction(ACTION_USB_PERMISSION)
            addAction(UsbManager.ACTION_USB_ACCESSORY_ATTACHED)
            addAction(UsbManager.ACTION_USB_ACCESSORY_DETACHED)
        }
        registerReceiver(usbReceiver, filter, flags)

        @Suppress("DEPRECATION")
        val fromIntent: UsbAccessory? = intent.getParcelableExtra(UsbManager.EXTRA_ACCESSORY)
        if (fromIntent != null) {
            Log.i(TAG, "launched via accessory-attached intent")
            openAccessory(fromIntent)
            return
        }

        Log.i(TAG, "no accessory in launch intent; checking accessoryList")
        val accessory = usbManager.accessoryList?.firstOrNull()
        if (accessory == null) {
            setStatus("No accessory attached. Waiting...")
            startPolling()
            return
        }
        requestOrOpen(accessory)
    }

    /**
     * Confirmed empirically: ACTION_USB_ACCESSORY_ATTACHED does not
     * reliably reach an already-running app on this device/OS (MagicOS) —
     * only a fresh Activity launch's accessoryList check picked up a
     * reattached device reliably. Polling accessoryList directly is the
     * mechanism that's actually proven to work, so it's the primary
     * reconnect path; the broadcast handler above is kept as a cheap,
     * possibly-faster fast path on devices where it does work, not relied
     * on alone.
     */
    private fun startPolling() {
        if (pollingActive) return
        pollingActive = true
        pollThread = Thread {
            while (pollingActive) {
                val accessory = usbManager.accessoryList?.firstOrNull()
                if (accessory != null) {
                    pollingActive = false
                    requestOrOpen(accessory)
                    return@Thread
                }
                Thread.sleep(1000)
            }
        }
        pollThread?.start()
    }

    private fun stopPolling() {
        pollingActive = false
    }

    private fun requestOrOpen(accessory: UsbAccessory) {
        if (usbManager.hasPermission(accessory)) {
            openAccessory(accessory)
        } else {
            setStatus("Requesting permission for $accessory")
            val pi = PendingIntent.getBroadcast(
                this, 0, Intent(ACTION_USB_PERMISSION),
                PendingIntent.FLAG_MUTABLE
            )
            usbManager.requestPermission(accessory, pi)
        }
    }

    private fun openAccessory(accessory: UsbAccessory) {
        stopPolling()
        closeConnection() // defensive: replace any stale connection first
        Log.i(TAG, "opening accessory: $accessory")
        val pfd = usbManager.openAccessory(accessory)
        if (pfd == null) {
            setStatus("openAccessory() returned null")
            Log.e(TAG, "openAccessory returned null")
            return
        }
        fd = pfd
        setStatus("Connected. Waiting for HELLO...")

        val input = FileInputStream(pfd.fileDescriptor)
        output = FileOutputStream(pfd.fileDescriptor)
        readerThread = Thread { runReadLoop(input) }
        readerThread?.start()
    }

    /** Tears down the current connection, if any. Safe to call when already closed. */
    private fun closeConnection() {
        readerThread?.interrupt()
        readerThread = null
        output = null
        try {
            fd?.close()
        } catch (e: Exception) {
            Log.e(TAG, "error closing fd", e)
        }
        fd = null
    }

    /**
     * The accessory's output stream is written from both the reader thread
     * (HELLO_ACK) and the UI thread (touch events) — synchronized so two
     * writes never interleave their bytes on the wire.
     */
    private fun sendFrame(bytes: ByteArray) {
        synchronized(outputLock) {
            try {
                output?.write(bytes)
            } catch (e: Exception) {
                Log.e(TAG, "error writing frame", e)
            }
        }
    }

    /**
     * No SurfaceView yet (Phase 3), so touches are captured against the
     * whole window and normalized by its size — matches what the wire
     * protocol expects (docs/wire-protocol.md's INPUT channel).
     */
    private fun handleTouch(view: View, event: MotionEvent): Boolean {
        val action = when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> TouchAction.DOWN
            MotionEvent.ACTION_MOVE -> TouchAction.MOVE
            MotionEvent.ACTION_UP -> TouchAction.UP
            MotionEvent.ACTION_CANCEL -> TouchAction.CANCEL
            else -> return false
        }
        val xNorm = (event.x / view.width).coerceIn(0f, 1f)
        val yNorm = (event.y / view.height).coerceIn(0f, 1f)
        val pressure = if (action == TouchAction.UP || action == TouchAction.CANCEL) 0f else event.pressure
        val touch = TouchEvent(action, pointerId = 0, x = xNorm, y = yNorm, pressure = pressure)
        sendFrame(encodeFrame(Channel.INPUT, Input.encodeTouch(touch)))
        return true
    }

    private fun runReadLoop(input: FileInputStream) {
        val reader = FrameReader()
        val buf = ByteArray(16384)
        try {
            while (true) {
                val n = input.read(buf)
                if (n < 0) {
                    Log.i(TAG, "EOF on accessory input stream")
                    break
                }
                reader.feed(buf, n)
                while (true) {
                    val frame = reader.tryReadFrame() ?: break
                    handleFrame(frame)
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "read loop error", e)
            setStatus("Error: ${e.message}. Waiting for reconnect...")
        }
        // Whether from EOF or an error, the connection is over; start
        // polling for a fresh accessory rather than sitting idle. Physical
        // unplugs also fire DETACHED (see usbReceiver), which calls this
        // redundantly but harmlessly (startPolling is idempotent).
        startPolling()
    }

    private fun handleFrame(frame: Frame) {
        when (frame.channel) {
            Channel.CONTROL -> {
                if (frame.payload.isNotEmpty() && (frame.payload[0].toInt() and 0xFF) == Control.HELLO) {
                    val hostVersion = frame.payload.getOrNull(1)?.toInt()?.and(0xFF) ?: 0
                    Log.i(TAG, "received HELLO, host protocol_version=$hostVersion")
                    sendFrame(encodeFrame(Channel.CONTROL, Control.encodeHelloAck(PROTOCOL_VERSION)))
                    setStatus("Connected\nHandshake complete (host v$hostVersion)")
                } else {
                    Log.w(TAG, "unexpected control payload: ${frame.payload.joinToString()}")
                }
            }
            else -> Log.w(TAG, "unexpected channel: ${frame.channel} (${frame.payload.size} bytes)")
        }
    }

    private fun setStatus(s: String) {
        runOnUiThread { statusView.text = s }
    }

    override fun onDestroy() {
        super.onDestroy()
        stopPolling()
        try {
            unregisterReceiver(usbReceiver)
        } catch (e: Exception) {
            // ignore
        }
        closeConnection()
    }
}
