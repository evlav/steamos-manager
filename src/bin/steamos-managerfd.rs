/*
 * Copyright © 2023 Collabora Ltd.
 *
 * SPDX-License-Identifier: MIT
 *
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 *
 * The above copyright notice and this permission notice shall be included
 * in all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

use dbus::MethodErr;
use std::os::fd::OwnedFd;
use dbus_tokio::connection;
use futures::future;
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use dbus_crossroads::Crossroads;

struct SteamOSManagerFD {
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>>
{
    // This daemon is responsible for creating a dbus api that steam client can use to do various OS
    // level things. It implements com.steampowered.SteamOSManagerFD interface
    let (resource, conn) = connection::new_system_sync()?;

    // The resource is a task that should be spawned onto a tokio compatible
    // reactor ASAP. If the resource ever finishes, you lost connection to D-Bus.
    //
    // To shut down the connection, both call _handle.abort() and drop the connection.
    let _handle = tokio::spawn(async {
        let err = resource.await;
        panic!("Lost connection to D-Bus: {}", err);
    });

    conn.request_name("com.steampowered.SteamOSManagerFD", false, true, false).await?;

    // Create a new crossroads instance.
    // The instance is configured so that introspection and properties interfaces
    // are added by default on object path additions.
    let mut cr = Crossroads::new();

    // Enable async support for the crossroads instance.
    cr.set_async_support(Some((conn.clone(), Box::new(|x| { tokio::spawn(x); }))));

    let manager = SteamOSManagerFD {};

    // Let's build a new interface, which can be used for "Hello" objects.
    let iface_token = cr.register("com.steampowered.SteamOSManagerFD", |b| {
        // Let's add a method to the interface. We have the method name, followed by
        // names of input and output arguments (used for introspection). The closure then controls
        // the types of these arguments. The last argument to the closure is a tuple of the input arguments.
        b.method_with_cr("GetAlsIntegrationTimeFileDescriptor", (), ("descriptor",), |_, _cr, (): ()| {
            // let hello: &mut Hello = cr.data_mut(ctx.path()).unwrap(); // ok_or_else(|| MethodErr::no_path(ctx.path()))?;
            // And here's what happens when the method is called.
            let result = std::fs::File::create("/sys/devices/platform/AMDI0010:00/i2c-0/i2c-PRP0001:01/iio:device0/in_illuminance_integration_time");
            // let s = format!("Hello {}! This API has been used {} times.", name, hello.called_count);
            match result {
                Ok(f) => { let fd: OwnedFd = OwnedFd::from(f); Ok((fd,)) },
                Err(message) => { println!("Unable to open file descriptor"); Err(MethodErr::failed(&message.to_string())) }

            }
            // async move {
                // And the return value is a tuple of the output arguments.
                // ctx.reply(Ok(result.))
                // The reply is sent when ctx is dropped / goes out of scope.
            // }
        });
    });
    
    cr.insert("/com/steampowered/SteamOSManagerFD", &[iface_token], manager);
    
    // We add the Crossroads instance to the connection so that incoming method calls will be handled.
    conn.start_receive(MatchRule::new_method_call(), Box::new(move |msg, conn| {
        cr.handle_message(msg, conn).unwrap();
        true
    }));
    
    future::pending::<()>().await;
    unreachable!()
}
