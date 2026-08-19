package ai.tabscreen.aoahello

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
import android.widget.TextView
import java.io.FileInputStream
import java.io.FileOutputStream

private const val TAG = "AoaHello"
private const val ECHO_MAX_BYTES = 64
private const val ACTION_USB_PERMISSION = "ai.tabscreen.aoahello.USB_PERMISSION"

class MainActivity : Activity() {
    private lateinit var statusView: TextView
    private lateinit var usbManager: UsbManager
    private var fd: ParcelFileDescriptor? = null
    private var readerThread: Thread? = null

    private val permissionReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action != ACTION_USB_PERMISSION) return
            val accessory: UsbAccessory? = intent.getParcelableExtra(UsbManager.EXTRA_ACCESSORY)
            val granted = intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
            Log.d(TAG, "permission result granted=$granted accessory=$accessory")
            if (granted && accessory != null) {
                openAccessory(accessory)
            } else {
                setStatus("Permission denied for accessory")
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        statusView = TextView(this)
        statusView.textSize = 20f
        statusView.setPadding(32, 64, 32, 32)
        setContentView(statusView)
        setStatus("Starting...")

        usbManager = getSystemService(USB_SERVICE) as UsbManager

        val flags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU)
            Context.RECEIVER_NOT_EXPORTED else 0
        registerReceiver(permissionReceiver, IntentFilter(ACTION_USB_PERMISSION), flags)

        val fromIntent: UsbAccessory? = intent.getParcelableExtra(UsbManager.EXTRA_ACCESSORY)
        if (fromIntent != null) {
            Log.d(TAG, "Launched via accessory-attached intent")
            openAccessory(fromIntent)
            return
        }

        Log.d(TAG, "No accessory in launch intent; checking accessoryList")
        val list = usbManager.accessoryList
        val accessory = list?.firstOrNull()
        if (accessory == null) {
            setStatus("No accessory attached (accessoryList empty)")
            Log.e(TAG, "accessoryList is empty")
            return
        }

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
        Log.d(TAG, "Opening accessory: $accessory")
        val pfd = usbManager.openAccessory(accessory)
        if (pfd == null) {
            setStatus("openAccessory() returned null")
            Log.e(TAG, "openAccessory returned null")
            return
        }
        fd = pfd
        setStatus("Accessory open. Waiting for data...")

        val input = FileInputStream(pfd.fileDescriptor)
        val output = FileOutputStream(pfd.fileDescriptor)

        readerThread = Thread {
            val buf = ByteArray(16384)
            var totalBytes = 0L
            var chunks = 0L
            val startNanos = System.nanoTime()
            var lastLogNanos = startNanos
            try {
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) {
                        Log.d(TAG, "EOF on accessory input stream")
                        break
                    }
                    totalBytes += n
                    chunks += 1

                    if (n <= ECHO_MAX_BYTES) {
                        // Latency probe: echo immediately.
                        output.write(buf, 0, n)
                    }

                    val now = System.nanoTime()
                    if (now - lastLogNanos > 1_000_000_000L) {
                        val elapsedSec = (now - startNanos) / 1e9
                        val mbPerSec = (totalBytes / (1024.0 * 1024.0)) / elapsedSec
                        Log.d(TAG, "THROUGHPUT totalBytes=$totalBytes chunks=$chunks elapsedSec=%.2f rate=%.2fMB/s".format(elapsedSec, mbPerSec))
                        runOnUiThread {
                            setStatus("Connected\nBytes: $totalBytes\nRate: %.2f MB/s".format(mbPerSec))
                        }
                        lastLogNanos = now
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "Reader thread error", e)
                runOnUiThread { setStatus("Error: ${e.message}") }
            }
            Log.d(TAG, "Reader thread exiting. totalBytes=$totalBytes chunks=$chunks")
        }
        readerThread?.start()
    }

    private fun setStatus(s: String) {
        runOnUiThread { statusView.text = s }
    }

    override fun onDestroy() {
        super.onDestroy()
        try {
            unregisterReceiver(permissionReceiver)
        } catch (e: Exception) {
            // ignore
        }
        try {
            fd?.close()
        } catch (e: Exception) {
            Log.e(TAG, "Error closing fd", e)
        }
    }
}
