// Copyright (c) 2026 Saccade contributors.

#import <AppKit/AppKit.h>

#include "tests/cefsimple/saccade_adapter.h"
#include "tests/cefsimple/saccade_agent_switch_mac.h"

static NSWindow* WindowForBrowser(CefRefPtr<CefBrowser> browser) {
  if (!browser) {
    return nil;
  }
  NSView* view = (__bridge NSView*)browser->GetHost()->GetWindowHandle();
  return view ? [view window] : nil;
}

void SaccadeUpdateAgentSwitch(CefRefPtr<CefBrowser> browser, int state) {
  // Kept as a compatibility symbol for pinned CEF patches. Agent state is
  // rendered by the same Chrome toolbar action used on Windows.
}

void SaccadeShowHumanVerificationFailure(CefRefPtr<CefBrowser> browser,
                                         const std::string& provider) {
  if (![NSThread isMainThread]) {
    dispatch_async(dispatch_get_main_queue(), ^{
      SaccadeShowHumanVerificationFailure(browser, provider);
    });
    return;
  }
  NSWindow* window = WindowForBrowser(browser);
  if (!window || !browser || !browser->IsValid()) {
    return;
  }
  NSString* providerName =
      [NSString stringWithUTF8String:provider.c_str()];
  if (!providerName || [providerName length] == 0) {
    providerName = @"the site's verification provider";
  }
  NSAlert* alert = [[NSAlert alloc] init];
  [alert setAlertStyle:NSAlertStyleWarning];
  [alert setMessageText:@"Human verification could not start"];
  [alert setInformativeText:[NSString
                                stringWithFormat:
                                    @"%@ rejected or could not create the verification session. "
                                     @"Saccade did not read or store challenge content, cookies, "
                                     @"or verification tokens.",
                                    providerName]];
  [alert addButtonWithTitle:@"Reload page"];
  [alert addButtonWithTitle:@"Not now"];
  [alert beginSheetModalForWindow:window
               completionHandler:^(NSModalResponse response) {
                 if (response == NSAlertFirstButtonReturn && browser &&
                     browser->IsValid()) {
                   SaccadeAdapter::GetInstance()->RetryHumanVerification(browser);
                 }
               }];
}

SaccadeProtectedValuePromptResult SaccadePromptProtectedValue(
    CefRefPtr<CefBrowser> browser,
    const std::string& page_origin,
    const std::string& field_label) {
  __block SaccadeProtectedValuePromptResult result;
  void (^showPrompt)(void) = ^{
    NSWindow* window = WindowForBrowser(browser);
    if (!window) {
      return;
    }
    NSString* origin = [NSString stringWithUTF8String:page_origin.c_str()];
    NSString* label = [NSString stringWithUTF8String:field_label.c_str()];
    if (!origin) {
      origin = @"unknown origin";
    }
    if (!label || [label length] == 0) {
      label = @"protected identifier";
    }

    NSAlert* alert = [[NSAlert alloc] init];
    [alert setAlertStyle:NSAlertStyleInformational];
    [alert setMessageText:@"Fill protected field locally"];
    [alert setInformativeText:[NSString
                                  stringWithFormat:
                                      @"Origin: %@\nPage field (untrusted label): %@\n\n"
                                       @"The value goes directly to this page. It is not sent "
                                       @"to the LLM, logs, screenshots, or replay.",
                                      origin, label]];
    [alert addButtonWithTitle:@"Fill locally"];
    [alert addButtonWithTitle:@"Cancel"];

    NSTextField* input =
        [[NSTextField alloc] initWithFrame:NSMakeRect(0, 0, 360, 24)];
    [input setPlaceholderString:@"Enter the protected value"];
    [input setAccessibilityLabel:@"Protected value (local only)"];
    [input setAccessibilityIdentifier:@"SaccadeProtectedValueInput"];
    [alert setAccessoryView:input];
    [[alert window] setInitialFirstResponder:input];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{
      [[alert window] makeFirstResponder:input];
    });
    [NSApp activateIgnoringOtherApps:YES];
    [window makeKeyAndOrderFront:nil];

    if ([alert runModal] == NSAlertFirstButtonReturn) {
      const char* utf8 = [[input stringValue] UTF8String];
      if (utf8 && utf8[0] != '\0') {
        result.confirmed = true;
        result.value = utf8;
      }
    }
    [input setStringValue:@""];
  };

  if ([NSThread isMainThread]) {
    showPrompt();
  } else {
    dispatch_sync(dispatch_get_main_queue(), showPrompt);
  }
  return result;
}
