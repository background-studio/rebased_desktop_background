package studio.background.rebased.bridge;

import com.intellij.openapi.editor.colors.TextAttributesKey;

import java.awt.Color;
import java.util.IdentityHashMap;
import java.util.Map;

import static studio.background.rebased.bridge.BridgeUnitFixtures.assertTrue;

/** Unit entry: TextContrast apply(Map) isolates by editor.getProject(); clear restores all. */
public final class TextContrastProjectIsolationTest {
  public static void main(String[] args) {
    BridgeUnitFixtures.Harness harness = new BridgeUnitFixtures.Harness().install();
    try {
      BridgeUnitFixtures.FakeProject projectA = new BridgeUnitFixtures.FakeProject("A");
      BridgeUnitFixtures.FakeProject projectB = new BridgeUnitFixtures.FakeProject("B");
      TextAttributesKey key = TextAttributesKey.createTextAttributesKey("TEST");
      BridgeUnitFixtures.FakeScheme schemeA = new BridgeUnitFixtures.FakeScheme()
        .background(new Color(0x101010))
        .put(key, new Color(0x808080));
      BridgeUnitFixtures.FakeScheme schemeB = new BridgeUnitFixtures.FakeScheme()
        .background(new Color(0x101010))
        .put(key, new Color(0x808080));
      BridgeUnitFixtures.FakeEditor editorA = harness.editorFactory.addEditor(projectA, schemeA);
      BridgeUnitFixtures.FakeEditor editorB = harness.editorFactory.addEditor(projectB, schemeB);

      TextContrast contrast = new TextContrast();
      Map<com.intellij.openapi.project.Project, Integer> onlyA = new IdentityHashMap<>();
      onlyA.put(projectA, 50);
      contrast.apply(onlyA);

      assertTrue(editorA.isLifted(), "project A editor should be lifted");
      assertTrue(!editorB.isLifted(), "project B editor should stay clear when absent from map");

      Color lifted = editorA.rawScheme().getAttributes(key).getForegroundColor();
      Color source = new Color(0x808080);
      assertTrue(!lifted.equals(source), "A foreground should change");

      // New editor under configured project gets wrapped after factory listener + invokeLater.
      BridgeUnitFixtures.FakeEditor editorA2 = harness.editorFactory.addEditor(projectA, new BridgeUnitFixtures.FakeScheme().put(key, source));
      harness.editorFactory.fireCreated(editorA2);
      harness.flushLater();
      assertTrue(editorA2.isLifted(), "new editor in project A should lift");

      // Theme switch rewraps configured projects only.
      harness.fireColorSchemeChanged();
      assertTrue(editorA.isLifted(), "A remains lifted after theme change");
      assertTrue(!editorB.isLifted(), "B remains clear after theme change");

      // Updating map without B still clears B; removing A clears A.
      Map<com.intellij.openapi.project.Project, Integer> onlyB = new IdentityHashMap<>();
      onlyB.put(projectB, 40);
      contrast.apply(onlyB);
      assertTrue(!editorA.isLifted(), "A cleared when removed from map");
      assertTrue(editorB.isLifted(), "B lifted when added");

      // Heartbeat with the same per-project map must not reinit any editor.
      Map<com.intellij.openapi.project.Project, Integer> both = new IdentityHashMap<>();
      both.put(projectA, 50);
      both.put(projectB, 40);
      contrast.apply(both);
      assertTrue(editorA.isLifted(), "A lifted in both map");
      assertTrue(editorB.isLifted(), "B lifted in both map");
      int reinitA = editorA.reinitCount();
      int reinitB = editorB.reinitCount();
      contrast.apply(new IdentityHashMap<>(both));
      assertTrue(editorA.reinitCount() == reinitA, "identical heartbeat must not reinit A");
      assertTrue(editorB.reinitCount() == reinitB, "identical heartbeat must not reinit B");

      // Changing only A must not reinit B.
      Map<com.intellij.openapi.project.Project, Integer> bumpA = new IdentityHashMap<>();
      bumpA.put(projectA, 60);
      bumpA.put(projectB, 40);
      contrast.apply(bumpA);
      assertTrue(editorA.reinitCount() == reinitA + 1, "A factor change reinits A");
      assertTrue(editorB.reinitCount() == reinitB, "A factor change must not reinit B");

      // Global apply still works and wraps all editors.
      contrast.apply(30);
      assertTrue(editorA.isLifted(), "global mode lifts A");
      assertTrue(editorB.isLifted(), "global mode lifts B");

      contrast.clear();
      assertTrue(!editorA.isLifted(), "clear restores A");
      assertTrue(!editorB.isLifted(), "clear restores B");

      System.out.println("TextContrastProjectIsolationTest: OK");
    }
    finally {
      harness.reset();
    }
  }
}
