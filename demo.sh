#!/bin/bash

echo "🚀 Nova v0.1.0 Demo - Wayland-Native Virtualization & Container Manager"
echo "=================================================================="
echo

echo "📋 Available commands:"
./zig-out/bin/nova
echo

echo "🔍 Listing current instances (should be empty):"
./zig-out/bin/nova ls
echo

echo "🐳 Starting a test container:"
./zig-out/bin/nova run container demo-api
echo

echo "💻 Starting a test VM:"
./zig-out/bin/nova run vm demo-vm
echo

echo "📊 Listing instances after creation:"
./zig-out/bin/nova ls
echo

echo "⏹️  Stopping the container:"
./zig-out/bin/nova stop container demo-api
echo

echo "⏹️  Stopping the VM:"
./zig-out/bin/nova stop vm demo-vm
echo

echo "✅ Demo complete! Nova v0.1.0 is ready for development."
echo "Next steps: Implement proper NovaFile parsing, persistent state, and GUI."