package se.denied.bastion.ssh

import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull

class SystemProbeTest {
    @Test
    fun parsesDashboardSnapshot() {
        val output = """
            @@LOADAVG
            0.12 0.34 0.56 1/100 42
            @@UPTIME
            12345.67 99.00
            @@MEM
            MemTotal:       8000000 kB
            MemAvailable:   3000000 kB
            @@DF
            Filesystem 1024-blocks Used Available Capacity Mounted on
            /dev/sda1 100000 40000 60000 40% /
            @@OS
            NAME=Example
            PRETTY_NAME="Example Linux 1"
            @@KERNEL
            Linux 6.12.0
            @@HOST
            bastion-test
            @@NPROC
            8
            @@DOCKER
            abc123|web|nginx:latest|Up 2 hours
            def456|worker|example:1|Exited (0) 1 hour ago
            @@END
        """.trimIndent()

        val snapshot = SystemProbe.parse(output)

        assertEquals("bastion-test", snapshot.hostname)
        assertEquals("Example Linux 1", snapshot.os)
        assertEquals("Linux 6.12.0", snapshot.kernel)
        assertEquals(8, snapshot.cpuCount)
        assertEquals(12345.67, snapshot.uptimeSeconds)
        assertEquals(LoadAverage(0.12, 0.34, 0.56), snapshot.load)
        assertEquals(8_000_000L * 1024, assertNotNull(snapshot.memory).totalBytes)
        assertEquals(3_000_000L * 1024, snapshot.memory?.availableBytes)
        assertEquals(40, assertNotNull(snapshot.rootDisk).capacityPercent)
        assertEquals(2, snapshot.containers.size)
        assertEquals("web", snapshot.containers.first().name)
    }

    @Test
    fun missingOptionalToolsYieldPartialSnapshot() {
        val snapshot = SystemProbe.parse("@@HOST\nminimal\n@@DOCKER\n@@END\n")

        assertEquals("minimal", snapshot.hostname)
        assertEquals(emptyList(), snapshot.containers)
        assertEquals(null, snapshot.memory)
        assertEquals(null, snapshot.rootDisk)
    }
}