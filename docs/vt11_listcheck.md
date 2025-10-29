session.startTransaction "VT11"
// choose variant popup
session.findById("wnd[0]").sendVKey 17
session.findById("wnd[1]").sendVKey 8
// yesterday as low date, tomorrow as high date in below format
session.findById("wnd[0]/usr/txtK_TPBEZ-LOW").text = "10/28/2025*"
session.findById("wnd[0]/usr/txtK_TPBEZ-HIGH").text = "10/30/2025*"
session.findById("wnd[0]/usr/txtK_TPBEZ-HIGH").setFocus
session.findById("wnd[0]/usr/txtK_TPBEZ-HIGH").caretPosition = 11
// execute
session.findById("wnd[0]").sendVKey 8
// choose layout
session.findById("wnd[0]/mbar/menu[3]/menu[0]/menu[1]").select
session.findById("wnd[1]/usr/lbl[1,22]").setFocus
session.findById("wnd[1]/usr/lbl[1,22]").caretPosition = 4
session.findById("wnd[1]").sendVKey 2
/\* click on first shipment number in list

- check for sbar
- if it goes to next page (into shipment)
- this is not being worked on, we go to next
- if we get sbar msg on delivery being processed, we make note of delivery and or shipment
- continue down list until last
  \*/
  session.findById("wnd[0]/usr/lbl[8,4]").setFocus
  session.findById("wnd[0]/usr/lbl[8,4]").caretPosition = 4
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
  session.findById("wnd[0]/usr/lbl[8,5]").setFocus
  session.findById("wnd[0]/usr/lbl[8,5]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
  session.findById("wnd[0]/usr/lbl[8,6]").setFocus
  session.findById("wnd[0]/usr/lbl[8,6]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
  session.findById("wnd[0]/usr/lbl[8,7]").setFocus
  session.findById("wnd[0]/usr/lbl[8,7]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]/usr/lbl[8,8]").setFocus
  session.findById("wnd[0]/usr/lbl[8,8]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]/usr/lbl[8,9]").setFocus
  session.findById("wnd[0]/usr/lbl[8,9]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
  session.findById("wnd[0]/usr/lbl[8,10]").setFocus
  session.findById("wnd[0]/usr/lbl[8,10]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
  session.findById("wnd[0]/usr/lbl[8,11]").setFocus
  session.findById("wnd[0]/usr/lbl[8,11]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
  session.findById("wnd[0]/usr/lbl[8,12]").setFocus
  session.findById("wnd[0]/usr/lbl[8,12]").caretPosition = 5
  session.findById("wnd[0]").sendVKey 2
  session.findById("wnd[0]").sendVKey 3
