const std = @import("std");

pub const LogLevel = enum {
    debug,
    info,
    warn,
    err,
};

var current_level: LogLevel = .info;

pub fn setLevel(level: LogLevel) void {
    current_level = level;
}

pub fn debug(comptime fmt: []const u8, args: anytype) void {
    if (@intFromEnum(current_level) <= @intFromEnum(LogLevel.debug)) {
        log(.debug, fmt, args);
    }
}

pub fn info(comptime fmt: []const u8, args: anytype) void {
    if (@intFromEnum(current_level) <= @intFromEnum(LogLevel.info)) {
        log(.info, fmt, args);
    }
}

pub fn warn(comptime fmt: []const u8, args: anytype) void {
    if (@intFromEnum(current_level) <= @intFromEnum(LogLevel.warn)) {
        log(.warn, fmt, args);
    }
}

pub fn err(comptime fmt: []const u8, args: anytype) void {
    if (@intFromEnum(current_level) <= @intFromEnum(LogLevel.err)) {
        log(.err, fmt, args);
    }
}

fn log(level: LogLevel, comptime fmt: []const u8, args: anytype) void {
    const timestamp = std.time.timestamp();
    const level_str = switch (level) {
        .debug => "DEBUG",
        .info => " INFO",
        .warn => " WARN",
        .err => "ERROR",
    };
    
    // Simple timestamp formatting
    const seconds_since_epoch = @as(u64, @intCast(timestamp));
    const seconds_in_day = seconds_since_epoch % (24 * 60 * 60);
    const hours = seconds_in_day / (60 * 60);
    const minutes = (seconds_in_day % (60 * 60)) / 60;
    const seconds = seconds_in_day % 60;
    
    std.debug.print("[{:02}:{:02}:{:02}] [{s}] " ++ fmt ++ "\n", .{ hours, minutes, seconds, level_str } ++ args);
}