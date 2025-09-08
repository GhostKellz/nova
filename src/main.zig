const std = @import("std");
const nova = @import("nova");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    if (args.len < 2) {
        try printHelp();
        return;
    }

    const command = args[1];
    
    if (std.mem.eql(u8, command, "run")) {
        if (args.len < 4) {
            std.debug.print("Usage: nova run <type> <name>\n", .{});
            return;
        }
        try nova.commands.run(allocator, args[2], args[3]);
    } else if (std.mem.eql(u8, command, "ls")) {
        try nova.commands.list(allocator);
    } else if (std.mem.eql(u8, command, "stop")) {
        if (args.len < 4) {
            std.debug.print("Usage: nova stop <type> <name>\n", .{});
            return;
        }
        try nova.commands.stop(allocator, args[2], args[3]);
    } else if (std.mem.eql(u8, command, "version")) {
        std.debug.print("Nova v0.1.0 - Wayland-Native Virtualization & Container Manager\n", .{});
    } else {
        std.debug.print("Unknown command: {s}\n", .{command});
        try printHelp();
    }
}

fn printHelp() !void {
    std.debug.print(
        \\Nova v0.1.0 - Wayland-Native Virtualization & Container Manager
        \\
        \\USAGE:
        \\    nova <COMMAND> [OPTIONS]
        \\
        \\COMMANDS:
        \\    run <type> <name>     Start a VM or container
        \\    ls                    List all running instances
        \\    stop <type> <name>    Stop a VM or container
        \\    version               Show version information
        \\
        \\EXAMPLES:
        \\    nova run vm win11     Start VM named 'win11'
        \\    nova run container api  Start container named 'api'
        \\    nova ls               List all running instances
        \\    nova stop vm win11    Stop VM named 'win11'
        \\
    , .{});
}