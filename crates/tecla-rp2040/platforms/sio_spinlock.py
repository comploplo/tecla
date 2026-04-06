# Minimal RP2040 SIO spinlock model for Renode HIL testing.
#
# The RP2040 SIO contains 32 hardware spinlocks at offsets 0x100..0x17C.
# Reading a spinlock register atomically claims it: returns non-zero if
# successfully claimed, zero if already held by someone else.
# Writing any value releases the spinlock.
#
# A plain MappedMemory always returns 0 on reads, making every spinlock
# appear permanently taken, which causes embassy-rp's critical-section
# implementation to spin forever.
#
# This script registers a Python peripheral to handle the spinlock range
# and implement the claim/release semantics correctly.

import sys

SPINLOCK_BASE = 0x100
SPINLOCK_COUNT = 32

# Track which spinlocks are held
locks = [False] * SPINLOCK_COUNT

def read_byte(offset):
    return read_word(offset) & 0xFF

def read_word(offset):
    if SPINLOCK_BASE <= offset < SPINLOCK_BASE + SPINLOCK_COUNT * 4:
        idx = (offset - SPINLOCK_BASE) // 4
        if locks[idx]:
            return 0  # already held
        locks[idx] = True
        return 1  # claimed successfully

    return 0

def write_byte(offset, value):
    write_word(offset, value)

def write_word(offset, value):
    if SPINLOCK_BASE <= offset < SPINLOCK_BASE + SPINLOCK_COUNT * 4:
        idx = (offset - SPINLOCK_BASE) // 4
        locks[idx] = False  # release

if 'self' in dir():
    self.read_byte = read_byte
    self.write_byte = write_byte
    self.read_word = read_word
    self.write_word = write_word
