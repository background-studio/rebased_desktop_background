package studio.background.rebased.bridge;

import com.intellij.openapi.application.impl.islands.IslandsUICustomization;
import com.intellij.openapi.wm.impl.IdeBackgroundUtil;
import javax.swing.*;
import java.util.function.Predicate;

public final class IslandBackgroundGateTest {
  private static final Object KEY = IdeBackgroundUtil.NO_BACKGROUND_PREDICATE;
  public static void main(String[] args) throws Exception {
    SwingUtilities.invokeAndWait(IslandBackgroundGateTest::verify);
    System.out.println("IslandBackgroundGateTest: panel coverage, window isolation, dynamic tabs and restoration passed");
  }
  @SuppressWarnings("unchecked")
  private static boolean blocked(JComponent component) {
    return ((Predicate<JComponent>)component.getClientProperty(KEY)).test(component);
  }
  private static void verify() {
    JPanel first = new JPanel(), second = new JPanel();
    JTable table = new JTable(2, 2), otherTable = new JTable(2, 2);
    first.add(table); second.add(otherTable);
    Predicate<JComponent> original = IslandsUICustomization.backgroundGate();
    table.putClientProperty(KEY, original); otherTable.putClientProperty(KEY, original);
    check(blocked(table), "Islands initially suppresses the opaque central table");
    IslandBackgroundGate gate = new IslandBackgroundGate();
    gate.refresh(first);
    check(!blocked(table), "central table must allow its native window painter");
    check(blocked(otherTable), "another window must remain unchanged");
    Object replacement = table.getClientProperty(KEY);
    JPanel renderer = new JPanel(); renderer.putClientProperty(KEY, original);
    CellRendererPane pane = new CellRendererPane(); first.add(pane); pane.add(renderer);
    var graphics = new java.awt.image.BufferedImage(4,4,2).createGraphics();
    try { com.intellij.util.ui.JBSwingUtilities.runGlobalCGTransform(renderer, graphics); }
    finally { graphics.dispose(); }
    check(!blocked(renderer), "late cell renderer must be unblocked during painting, without waiting for heartbeat");
    check(Boolean.TRUE.equals(renderer.getClientProperty("test.nativeTransformReplayed")), "late renderer needs native graphics replay");
    gate.refresh(first);
    check(table.getClientProperty(KEY) == replacement, "heartbeat must not stack wrappers");
    table.putClientProperty(IdeBackgroundUtil.NO_BACKGROUND, true);
    check(blocked(table), "explicit component opt-out must win");
    table.putClientProperty(IdeBackgroundUtil.NO_BACKGROUND, null);
    JTree added = new JTree(); added.putClientProperty(KEY, original); first.add(added);
    JPanel custom = new JPanel(); Predicate<JComponent> customGate = component -> true;
    custom.putClientProperty(KEY, customGate); first.add(custom);
    gate.refresh(first);
    check(!blocked(added), "new tabs must receive background coverage");
    check(custom.getClientProperty(KEY) == customGate, "unrelated predicates must not be replaced");
    first.remove(added); gate.refresh(first);
    check(added.getClientProperty(KEY) == original, "detached content must restore original predicate");
    gate.clear();
    check(com.intellij.util.ui.JBSwingUtilities.registrations() == 0, "clear must remove the window transform");
    check(table.getClientProperty(KEY) == original, "pause/clear must restore exact original predicate");
    gate.refresh(first); table.putClientProperty(KEY, customGate); gate.clear();
    check(table.getClientProperty(KEY) == customGate, "clear must not overwrite later theme/plugin changes");
  }
  private static void check(boolean value, String message) { if (!value) throw new AssertionError(message); }
}
